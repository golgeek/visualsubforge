import { untrack } from 'svelte';
import {
  timingStartError,
  timingProposalWatch,
  timingSlotWait,
  timingSlotCancelWait,
} from '../client';
import type {
  TimingFixView,
  DocumentSnapshot,
  ReviewSelection,
} from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
type Review = Pick<TimingFixView, 'phase' | 'changed' | 'error'> &
  Partial<Pick<TimingFixView, 'coverage' | 'selectionError'>>;
interface Client<V, S> {
  purpose: 'fix' | 'batch';
  start(
    session: number,
    document: DocumentSnapshot,
    selectedCueId: S,
    selection: ReviewSelection | null,
  ): Promise<number>;
  view(
    id: number,
    offset: number,
    limit: number,
    previewIds?: readonly number[] | null,
  ): Promise<V>;
  cancel(id: number): Promise<void>;
  accept(id: number, document: DocumentSnapshot): Promise<boolean>;
}

export const LIVE_REVIEW_DELAY_MS = 250;
import type { WorkspaceActivity } from '../workspace/workspace-activity.svelte';
export interface LiveTimingReview {
  activity?: WorkspaceActivity;
  retainDisplay?: boolean;
  enabled: () => boolean;
  selection: () => ReviewSelection | null;
  context: () => string;
  previewIds: () => readonly number[];
  previewEnabled?: () => boolean;
  initialOffset?: () => number;
  pageKey?: () => string;
}
const selectionKey = (selection: ReviewSelection | null) =>
  JSON.stringify(selection);

// Presentation lifetime and bounded projections only. Rust owns the immutable proposal;
// DocumentSession reconciles its accepted reply with ordinary document operations.
export class TimingRepairSession<V extends Review, S = string | null> {
  view = $state.raw<V | null>(null);
  preview = $state.raw<V | null>(null);
  error = $state('');
  pending = $state(false);
  accepting = $state(false);
  applyingDraft = $state(false);
  private retained = $state.raw<{
    document: DocumentSnapshot;
    session: number;
    context: string;
    view: V;
    preview: V | null;
  } | null>(null);
  get displayed() {
    const last = this.retained;
    return last &&
      last.document.generation === this.subtitles.snapshot.generation &&
      last.session === this.mediaSession() &&
      last.context === this.live?.context()
      ? last
      : null;
  }
  private endActivity: (() => void) | undefined;
  private alive = true;
  private epoch = 0;
  private id: number | null = null;
  private capture = $state.raw<{
    document: DocumentSnapshot;
    session: number;
    selectedCueId: S;
    selection: string;
    context: string;
  } | null>(null);
  private polling = false;
  private offset = 0;
  private pageKey: string | undefined;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private liveSignature = '';
  private liveDocument: DocumentSnapshot | null = null;
  private viewportKey = '';
  private liveDirty = false;
  private liveWanted = false;
  private attempts = 0;
  private admissionRetryable = false;
  private get superseded() {
    return this.live?.retainDisplay && this.liveDirty && !this.current;
  }
  private watchSequence = 0;
  private watch: { ticket: number | null } | null = null;
  private released = $state(false);
  private readRequested = false;
  private driveOnIdle = false;
  private previewKey = '';

  constructor(
    private subtitles: DocumentSession,
    private mediaSession: () => number,
    private api: Client<V, S>,
    private scope: () => S,
    private allowedCoverage: readonly string[],
    private sameScope: (a: S, b: S) => boolean = Object.is,
    private live?: LiveTimingReview,
  ) {
    if (live)
      $effect(() => {
        const document = subtitles.snapshot;
        const enabled = (live.previewEnabled ?? live.enabled)();
        const signature = JSON.stringify([
          subtitles.snapshot.generation,
          subtitles.snapshot.revision,
          mediaSession(),
          scope(),
          live.selection(),
          live.context(),
          enabled,
          subtitles.dirty,
        ]);
        const wanted = enabled && !subtitles.dirty;
        // Viewport changes only refresh a bounded projection, never the proposal.
        const ids = JSON.stringify(live.previewIds().slice(0, 200));
        untrack(() => {
          if (
            signature === this.liveSignature &&
            ids === this.viewportKey &&
            document === this.liveDocument
          )
            return;
          const opening = this.liveDocument === null;
          this.viewportKey = ids;
          if (
            signature !== this.liveSignature ||
            document !== this.liveDocument
          ) {
            if (!this.displayed) this.retained = null;
            if (live.retainDisplay) {
              this.endActivity?.();
              this.endActivity = wanted ? live.activity?.begin() : undefined;
            }
            this.liveDocument = document;
            this.liveSignature = signature;
            this.liveDirty = true;
            this.attempts = 0;
            this.preview = null;
            this.previewKey = '';
            this.error = '';
          }
          this.liveWanted = wanted;
          this.schedule(
            opening || live.retainDisplay ? 0 : LIVE_REVIEW_DELAY_MS,
          );
        });
      });
  }
  get previewDocument() {
    return this.capture?.document ?? null;
  }
  private schedule(delay: number) {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.drive();
    }, delay);
  }
  /** Latest input only, with bounded retries for read-only admission while a
   * retired worker drains. Mutations are never retried here. */
  private async drive() {
    if (!this.alive || !this.live) return;
    if (this.pending || this.polling || this.accepting) {
      this.driveOnIdle = true;
      return;
    }
    const delay = this.live.retainDisplay ? 0 : LIVE_REVIEW_DELAY_MS;
    const signature = this.liveSignature;
    const document = this.liveDocument;
    if (this.readRequested && !this.liveDirty) {
      this.readRequested = false;
      await this.poll();
    }
    if (!this.liveWanted) {
      if (this.id !== null) await this.discard();
      this.preview = null;
      return;
    }
    if (this.liveDirty) {
      if (this.id !== null) {
        await this.discard();
        if (!this.alive) return;
        this.schedule(delay);
        return;
      }
      if (!this.canStart) {
        this.schedule(LIVE_REVIEW_DELAY_MS);
        return;
      }
      this.admissionRetryable = false;
      const start = this.start();
      await (this.live.activity && !this.live.retainDisplay
        ? this.live.activity.track(start)
        : start);
      if (!this.alive) return;
      if (signature !== this.liveSignature || document !== this.liveDocument) {
        this.schedule(delay);
        return;
      }
      if (this.id === null) {
        const retryable = !this.live.retainDisplay || this.admissionRetryable;
        if (retryable && ++this.attempts < 3)
          this.schedule(LIVE_REVIEW_DELAY_MS);
        else {
          this.liveDirty = false;
          this.endActivity?.();
        }
        return;
      }
      this.liveDirty = false;
    }
    if (!this.alive) return;
    if (signature !== this.liveSignature || document !== this.liveDocument) {
      this.schedule(delay);
      return;
    }
    if (this.current && this.view?.phase === 'reviewing' && !this.released) {
      if (this.id !== null) void this.watchProposal(this.id, 'released');
      await this.readPreview();
    } else {
      this.preview = null;
      this.previewKey = '';
    }
    if (
      signature === this.liveSignature &&
      document === this.liveDocument &&
      !this.running
    )
      this.endActivity?.();
    if (this.alive && this.running && this.id !== null)
      void this.watchProposal(this.id, 'finished');
  }
  private resumeDrive() {
    if (
      this.driveOnIdle &&
      this.alive &&
      !this.pending &&
      !this.polling &&
      !this.accepting
    ) {
      this.driveOnIdle = false;
      this.schedule(0);
    }
  }
  private withdrawWatch() {
    ++this.watchSequence;
    const ticket = this.watch?.ticket;
    this.watch = null;
    this.readRequested = false;
    if (ticket != null) void timingSlotCancelWait(ticket).catch(() => {});
  }

  private async watchProposal(id: number, signal: 'finished' | 'released') {
    if (this.watch) return;
    const sequence = this.watchSequence;
    try {
      if (
        !(await this.waitForSignal(id, signal)) ||
        sequence !== this.watchSequence
      )
        return;
      // Gate the old proposal before dispatching or awaiting its final read.
      if (signal === 'released') this.released = true;
      this.readRequested = true;
      this.schedule(0);
    } catch (error) {
      if (sequence !== this.watchSequence) return;
      this.released = true;
      this.error = String(error);
      this.endActivity?.();
    }
  }
  private async waitForSignal(id: number, signal: 'finished' | 'released') {
    const watch = { ticket: null as number | null };
    this.watch = watch;
    try {
      const ticket = await timingProposalWatch(id, this.api.purpose, signal);
      if (this.watch !== watch) {
        if (ticket !== null) await timingSlotCancelWait(ticket);
        return false;
      }
      watch.ticket = ticket;
      if (ticket !== null) await timingSlotWait(ticket);
      return this.watch === watch;
    } catch (error) {
      if (this.watch !== watch) return false;
      throw error;
    } finally {
      if (this.watch === watch) this.watch = null;
    }
  }

  private async readPreview() {
    if (
      !this.live ||
      this.id === null ||
      this.pending ||
      this.polling ||
      this.accepting
    )
      return;
    const ids = this.live.previewIds().slice(0, 200);
    const key = JSON.stringify([this.id, ids]);
    if (key === this.previewKey) return;
    const epoch = this.epoch;
    this.polling = true;
    try {
      const view = await this.api.view(this.id, 0, 200, ids);
      if (
        !this.active(epoch) ||
        !this.current ||
        this.released ||
        key !== JSON.stringify([this.id, this.live.previewIds().slice(0, 200)])
      )
        return;
      this.preview = view.phase === 'reviewing' ? view : null;
      if (this.retained && this.preview)
        this.retained = { ...this.retained, preview: this.preview };
      this.previewKey = key;
    } catch (error) {
      if (this.active(epoch) && !this.superseded) {
        this.preview = null;
        this.error = String(error);
      }
    } finally {
      this.polling = false;
      this.resumeDrive();
    }
  }
  retry() {
    if (!this.alive || this.accepting) return;
    this.error = '';
    this.liveDirty = true;
    this.attempts = 0;
    if (this.live?.retainDisplay) {
      this.endActivity?.();
      this.endActivity = this.live.activity?.begin();
    }
    if (this.live) this.schedule(0);
    else void this.start();
  }
  get running() {
    return (
      this.id !== null &&
      (!this.view || ['preparing', 'cancelling'].includes(this.view.phase))
    );
  }
  get canClose() {
    return !this.accepting && !this.applyingDraft;
  }
  get canStart() {
    return this.alive && !this.pending && !this.accepting && !this.running;
  }
  get current() {
    return (
      !!this.capture &&
      this.capture.document === this.subtitles.snapshot &&
      !this.subtitles.dirty &&
      this.capture.session === this.mediaSession() &&
      this.sameScope(this.capture.selectedCueId, this.scope()) &&
      this.capture.selection === selectionKey(this.live?.selection() ?? null) &&
      this.capture.context === (this.live?.context() ?? '')
    );
  }
  get canAccept() {
    return (
      !this.released &&
      this.canStart &&
      (!this.live || this.live.enabled()) &&
      this.current &&
      this.view?.phase === 'reviewing' &&
      this.view.changed > 0 &&
      !this.view.selectionError &&
      (!this.live || !this.error) &&
      this.eligible(this.view)
    );
  }
  protected eligible(view: V) {
    return this.allowedCoverage.includes(view.coverage ?? '');
  }
  private active(epoch: number) {
    return this.alive && epoch === this.epoch;
  }
  async applyDraft() {
    if (!this.canStart) return;
    this.pending = true;
    this.applyingDraft = true;
    this.error = '';
    try {
      await this.subtitles.apply();
    } catch (e) {
      if (this.alive) this.error = String(e);
    } finally {
      if (this.alive) {
        this.pending = false;
        this.applyingDraft = false;
        this.resumeDrive();
      }
    }
  }
  async start() {
    if (
      !this.canStart ||
      this.subtitles.dirty ||
      !this.subtitles.snapshot.cues.length
    )
      return;
    const selection = this.live?.selection() ?? null;
    const capture = {
      selection: selectionKey(selection),
      context: this.live?.context() ?? '',
      document: this.subtitles.snapshot,
      session: this.mediaSession(),
      selectedCueId: this.scope(),
    };
    this.withdrawWatch();
    this.released = false;
    const epoch = ++this.epoch;
    this.pending = true;
    this.error = '';
    this.view = null;
    this.preview = null;
    this.previewKey = '';
    const key = this.live?.pageKey?.();
    if (key === undefined || key !== this.pageKey)
      this.offset = this.live?.initialOffset?.() ?? 0;
    this.pageKey = key;
    try {
      if (this.id !== null) await this.api.cancel(this.id);
      if (!this.active(epoch)) return;
      this.id = null;
      this.capture = capture;
      const id = await this.api.start(
        capture.session,
        capture.document,
        capture.selectedCueId,
        selection,
      );
      if (!this.active(epoch)) {
        await this.api.cancel(id).catch(() => {});
        return;
      }
      this.id = id;
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) {
        if (this.id !== null) void this.api.cancel(this.id).catch(() => {});
        const refusal = timingStartError(e);
        this.admissionRetryable =
          refusal?.kind === 'workerBusy' || refusal?.kind === 'policyChanged';
        if (
          !this.superseded &&
          (!this.live?.retainDisplay ||
            !this.admissionRetryable ||
            this.attempts >= 2)
        )
          this.error = refusal?.message ?? String(e);
        this.id = null;
        this.capture = null;
      }
    } finally {
      if (this.active(epoch)) this.pending = false;
      this.resumeDrive();
    }
  }
  private async read(epoch: number) {
    if (this.id === null) return;
    const view = await this.api.view(this.id, this.offset, 100);
    if (!this.active(epoch)) return;
    if (!this.current) {
      const reason =
        this.capture?.document !== this.subtitles.snapshot
          ? 'Subtitles changed'
          : this.subtitles.dirty
            ? 'Cue draft changed'
            : this.capture?.session !== this.mediaSession()
              ? 'Media changed'
              : 'Selection or scope changed';
      await this.api.cancel(this.id).catch(() => {});
      if (!this.active(epoch)) return;
      this.view = null;
      this.id = null;
      this.capture = null;
      if (!this.live?.retainDisplay || !this.liveDirty)
        this.error = `${reason}; preview again.`;
      return;
    }
    if (this.released && view.phase === 'reviewing') return;
    this.view = view;
    if (this.live?.retainDisplay && this.capture) {
      if (view.phase === 'reviewing') {
        this.retained = {
          document: this.capture.document,
          session: this.capture.session,
          context: this.capture.context,
          view,
          preview: this.displayed?.preview ?? null,
        };
      } else if (view.phase === 'stale' || view.phase === 'closed')
        this.retained = null;
    }
    if (view.error) this.error = view.error;
  }
  async poll() {
    if (
      !this.alive ||
      this.pending ||
      this.accepting ||
      this.polling ||
      this.id === null
    )
      return;
    // A signalled worker or released owner gets one status read.
    this.polling = true;
    const epoch = this.epoch;
    try {
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) {
        if (this.id !== null) void this.api.cancel(this.id).catch(() => {});
        if (!this.superseded) this.error = String(e);
        this.view = null;
        this.id = null;
        this.capture = null;
      }
    } finally {
      this.polling = false;
      this.resumeDrive();
    }
  }
  async page(offset: number) {
    if (!this.canStart || this.view?.phase !== 'reviewing') return;
    this.offset = offset;
    this.pending = true;
    const epoch = ++this.epoch;
    try {
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch) && !this.superseded) this.error = String(e);
    } finally {
      if (this.active(epoch)) this.pending = false;
      this.resumeDrive();
    }
  }
  // A changed context needs a new proposal. Reading the cancelled page cannot inform it.
  private async discard() {
    const id = this.id;
    if (id === null) return;
    this.withdrawWatch();
    const epoch = ++this.epoch;
    this.pending = true;
    this.released = true;
    const running = this.running;
    try {
      await this.api.cancel(id);
      // Admission cannot replace a draining worker. Wait for its actual finish.
      if (running && this.active(epoch))
        await this.waitForSignal(id, 'finished');
    } catch (error) {
      if (this.active(epoch) && !this.superseded) this.error = String(error);
    } finally {
      if (this.active(epoch)) {
        this.pending = false;
        this.id = null;
        this.view = null;
        this.capture = null;
        this.resumeDrive();
      }
    }
  }
  async accept(): Promise<boolean> {
    if (!this.canAccept || this.id === null || !this.capture) return false;
    ++this.epoch;
    this.accepting = true;
    this.error = '';
    try {
      const accepted = await this.api.accept(this.id, this.capture.document);
      return this.alive && accepted;
    } catch (e) {
      if (this.alive) this.error = String(e);
      return false;
    } finally {
      if (this.alive) this.accepting = false;
      this.resumeDrive();
    }
  }
  dispose() {
    if (!this.alive) return;
    this.alive = false;
    this.withdrawWatch();
    this.endActivity?.();
    this.retained = null;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.preview = null;
    ++this.epoch;
    if (this.id !== null) void this.api.cancel(this.id).catch(() => {});
  }
}
