import { untrack } from 'svelte';
import type {
  SceneRepairView,
  DocumentSnapshot,
  ReviewSelection,
} from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
type Review = Pick<SceneRepairView, 'phase' | 'changed' | 'error'> &
  Partial<Pick<SceneRepairView, 'coverage' | 'selectionError'>>;
interface Client<V, S> {
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
export interface LiveTimingReview {
  enabled: () => boolean;
  selection: () => ReviewSelection | null;
  context: () => string;
  previewIds: () => readonly number[];
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
  private timer: ReturnType<typeof setTimeout> | null = null;
  private liveSignature = '';
  private liveDocument: DocumentSnapshot | null = null;
  private viewportKey = '';
  private liveDirty = false;
  private liveWanted = false;
  private attempts = 0;
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
        const signature = JSON.stringify([
          subtitles.snapshot.generation,
          subtitles.snapshot.revision,
          mediaSession(),
          scope(),
          live.selection(),
          live.context(),
          live.enabled(),
          subtitles.dirty,
        ]);
        const wanted = live.enabled() && !subtitles.dirty;
        // Viewport changes only refresh a bounded projection, never the proposal.
        const ids = JSON.stringify(live.previewIds().slice(0, 200));
        untrack(() => {
          if (
            signature === this.liveSignature &&
            ids === this.viewportKey &&
            document === this.liveDocument
          )
            return;
          this.viewportKey = ids;
          if (
            signature !== this.liveSignature ||
            document !== this.liveDocument
          ) {
            this.liveDocument = document;
            this.liveSignature = signature;
            this.liveDirty = true;
            this.attempts = 0;
            this.preview = null;
            this.previewKey = '';
            this.error = '';
          }
          this.liveWanted = wanted;
          this.schedule(LIVE_REVIEW_DELAY_MS);
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
      this.schedule(100);
      return;
    }
    const signature = this.liveSignature;
    const document = this.liveDocument;
    if (!this.liveWanted) {
      if (this.id !== null && this.view?.phase !== 'cancelled')
        await this.cancel();
      this.preview = null;
      if (this.running) this.schedule(LIVE_REVIEW_DELAY_MS);
      return;
    }
    if (this.liveDirty) {
      if (
        this.id !== null &&
        (!this.view ||
          !['cancelled', 'stale', 'failed', 'closed', 'accepted'].includes(
            this.view.phase,
          ))
      ) {
        await this.cancel();
        if (this.alive) this.schedule(LIVE_REVIEW_DELAY_MS);
        return;
      }
      if (!this.canStart) {
        this.schedule(LIVE_REVIEW_DELAY_MS);
        return;
      }
      await this.start();
      if (!this.alive) return;
      if (signature !== this.liveSignature || document !== this.liveDocument) {
        this.schedule(LIVE_REVIEW_DELAY_MS);
        return;
      }
      if (this.id === null) {
        if (++this.attempts < 3) this.schedule(LIVE_REVIEW_DELAY_MS);
        else this.liveDirty = false;
        return;
      }
      this.liveDirty = false;
    } else await this.poll();
    if (!this.alive) return;
    if (signature !== this.liveSignature || document !== this.liveDocument) {
      this.schedule(LIVE_REVIEW_DELAY_MS);
      return;
    }
    if (this.current && this.view?.phase === 'reviewing')
      await this.readPreview();
    else {
      this.preview = null;
      this.previewKey = '';
    }
    if (this.alive && this.id !== null) this.schedule(400);
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
        key !== JSON.stringify([this.id, this.live.previewIds().slice(0, 200)])
      )
        return;
      this.preview = view.phase === 'reviewing' ? view : null;
      this.previewKey = key;
    } catch (error) {
      if (this.active(epoch)) {
        this.preview = null;
        this.error = String(error);
      }
    } finally {
      this.polling = false;
    }
  }
  retry() {
    if (!this.alive || this.accepting) return;
    this.error = '';
    this.liveDirty = true;
    this.attempts = 0;
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
    const epoch = ++this.epoch;
    this.pending = true;
    this.error = '';
    this.view = null;
    this.preview = null;
    this.previewKey = '';
    this.offset = 0;
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
        this.error = String(e);
        this.id = null;
        this.capture = null;
      }
    } finally {
      if (this.active(epoch)) this.pending = false;
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
      this.error = `${reason}; preview again.`;
      return;
    }
    this.view = view;
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
    // Poll completed proposals too: scene/profile changes are Rust-owned.
    this.polling = true;
    const epoch = this.epoch;
    try {
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) {
        if (this.id !== null) void this.api.cancel(this.id).catch(() => {});
        this.error = String(e);
        this.view = null;
        this.id = null;
        this.capture = null;
      }
    } finally {
      this.polling = false;
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
      if (this.active(epoch)) this.error = String(e);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  async cancel() {
    if (!this.alive || this.id === null || this.pending || this.accepting)
      return;
    const epoch = ++this.epoch;
    this.pending = true;
    this.error = '';
    try {
      await this.api.cancel(this.id);
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) this.error = String(e);
    } finally {
      if (this.active(epoch)) this.pending = false;
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
    }
  }
  dispose() {
    if (!this.alive) return;
    this.alive = false;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.preview = null;
    ++this.epoch;
    if (this.id !== null) void this.api.cancel(this.id).catch(() => {});
  }
}
