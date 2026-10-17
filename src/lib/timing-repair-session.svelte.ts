import type { SceneRepairView, DocumentSnapshot } from './client';
import type { DocumentSession } from './document-session.svelte';
type Review = Pick<SceneRepairView, 'phase' | 'changed' | 'coverage' | 'error'>;
interface Client<V> {
  start(
    session: number,
    document: DocumentSnapshot,
    selectedCueId: string | null,
  ): Promise<number>;
  view(id: number, offset: number, limit: number): Promise<V>;
  cancel(id: number): Promise<void>;
  accept(id: number, document: DocumentSnapshot): Promise<boolean>;
}

// Dialog lifetime and bounded projections only. Rust owns the immutable proposal;
// DocumentSession reconciles its accepted reply with ordinary document operations.
export class TimingRepairSession<V extends Review> {
  view = $state.raw<V | null>(null);
  error = $state('');
  pending = $state(false);
  accepting = $state(false);
  applyingDraft = $state(false);
  private alive = true;
  private epoch = 0;
  private id: number | null = null;
  private capture: {
    document: DocumentSnapshot;
    session: number;
    selectedCueId: string | null;
  } | null = null;
  private polling = false;
  private offset = 0;
  constructor(
    private subtitles: DocumentSession,
    private mediaSession: () => number,
    private api: Client<V>,
    private scope: () => string | null,
    private allowedCoverage: readonly string[],
  ) {}
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
      this.capture.selectedCueId === this.scope()
    );
  }
  get canAccept() {
    return (
      this.canStart &&
      this.current &&
      this.view?.phase === 'reviewing' &&
      this.view.changed > 0 &&
      this.allowedCoverage.includes(this.view.coverage ?? '')
    );
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
    const capture = {
      document: this.subtitles.snapshot,
      session: this.mediaSession(),
      selectedCueId: this.scope(),
    };
    const epoch = ++this.epoch;
    this.pending = true;
    this.error = '';
    this.view = null;
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
      await this.api.cancel(this.id).catch(() => {});
      if (!this.active(epoch)) return;
      this.view = null;
      this.id = null;
      this.capture = null;
      this.error =
        'Workspace, selection, scope or cue draft changed; preview again.';
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
    ++this.epoch;
    if (this.id !== null) void this.api.cancel(this.id).catch(() => {});
  }
}
