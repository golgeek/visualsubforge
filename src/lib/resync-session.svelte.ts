import {
  chooseFile,
  resyncStart,
  resyncView,
  resyncCancel,
  type DocumentSnapshot,
  type MediaStatus,
  type ResyncView,
} from './client';
import type { DocumentSession } from './document-session.svelte';

const client = { chooseFile, resyncStart, resyncView, resyncCancel };
type Capture = { document: DocumentSnapshot; session: number };

// Dialog lifetime, projections and request ordering only. Rust owns the job and
// immutable proposal; DocumentSession alone adopts accepted document replies.
export class ResyncSession {
  source = $state('');
  view = $state.raw<ResyncView | null>(null);
  error = $state('');
  pending = $state(false);
  accepting = $state(false);
  applyingDraft = $state(false);
  cancelling = $state(false);
  private alive = true;
  private epoch = 0;
  private id: number | null = null;
  private capture: Capture | null = null;
  private polling = false;
  private cueOffset = 0;
  private sectionOffset = 0;

  constructor(
    private subtitles: DocumentSession,
    private media: () => MediaStatus,
    private api = client,
  ) {}

  get running() {
    return (
      (this.id !== null && this.view === null) ||
      this.view?.phase === 'analyzing' ||
      this.view?.phase === 'cancelling' ||
      this.view?.phase === 'accepting'
    );
  }
  get canClose() {
    // Keep the workspace inaccessible until a document mutation acknowledges;
    // otherwise typing after modal close could be overwritten by its reply.
    return !this.accepting && !this.applyingDraft;
  }
  get canCancel() {
    return (
      this.alive &&
      !this.pending &&
      !this.cancelling &&
      (this.running || this.accepting)
    );
  }
  get canStart() {
    return (
      this.alive &&
      !this.pending &&
      !this.accepting &&
      !this.cancelling &&
      !this.running
    );
  }
  get canAccept() {
    return (
      this.canStart &&
      this.view?.phase === 'reviewing' &&
      this.view.complete &&
      this.current()
    );
  }
  private current() {
    return (
      !!this.capture &&
      this.capture.document === this.subtitles.snapshot &&
      !this.subtitles.dirty &&
      this.media().loaded &&
      this.capture.session === this.media().session
    );
  }
  private active(epoch: number) {
    return this.alive && epoch === this.epoch;
  }
  async chooseSource() {
    if (!this.canStart) return;
    this.pending = true;
    const epoch = ++this.epoch;
    try {
      const path = await this.api.chooseFile('media', this.source || null);
      if (!this.active(epoch) || !path || path === this.source) return;
      // Changing an input retires any reviewed proposal before another start.
      if (this.id !== null) await this.api.resyncCancel(this.id);
      if (!this.active(epoch)) return;
      this.id = null;
      this.capture = null;
      this.view = null;
      this.source = path;
      this.error = '';
    } catch (error) {
      if (this.active(epoch)) this.error = String(error);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  async applyDraft() {
    if (!this.canStart) return;
    this.pending = true;
    this.applyingDraft = true;
    this.error = '';
    try {
      await this.subtitles.apply();
    } catch (error) {
      if (this.alive) this.error = String(error);
    } finally {
      if (this.alive) {
        this.pending = false;
        this.applyingDraft = false;
      }
    }
  }
  async start() {
    if (!this.canStart) return;
    const document = this.subtitles.snapshot;
    const media = this.media();
    if (
      !this.source ||
      !document.cues.length ||
      !media.loaded ||
      this.subtitles.dirty
    ) {
      this.error =
        'Choose a source video and resolve the cue draft before analysis.';
      return;
    }
    this.pending = true;
    this.error = '';
    this.view = null;
    this.id = null;
    this.cueOffset = this.sectionOffset = 0;
    this.capture = { document, session: media.session };
    const epoch = ++this.epoch;
    try {
      const id = await this.api.resyncStart(
        this.source,
        media.session,
        document,
      );
      if (!this.active(epoch) || !this.current()) {
        // Closing during start cannot leave an orphaned backend job.
        await this.api.resyncCancel(id);
        if (this.active(epoch))
          this.error = 'Subtitles or target video changed; analyze again.';
        return;
      }
      this.id = id;
      await this.read(epoch);
    } catch (error) {
      if (this.active(epoch)) this.error = String(error);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  private async read(epoch: number) {
    if (this.id === null || !this.active(epoch)) return;
    if (!this.current()) {
      await this.api.resyncCancel(this.id);
      if (this.active(epoch)) {
        this.view = null;
        this.error = 'Subtitles or target video changed; analyze again.';
        this.id = null;
      }
      return;
    }
    const view = await this.api.resyncView(
      this.id,
      this.cueOffset,
      this.sectionOffset,
    );
    if (!this.active(epoch)) return;
    if (!this.current()) {
      await this.api.resyncCancel(view.id);
      if (this.active(epoch)) {
        this.view = null;
        this.error = 'Subtitles or target video changed; analyze again.';
        this.id = null;
      }
      return;
    }
    this.view = view;
    this.error = view.error ?? '';
    if (
      view.phase !== 'analyzing' &&
      view.phase !== 'cancelling' &&
      view.phase !== 'accepting'
    )
      this.cancelling = false;
  }
  async poll() {
    if (
      !this.alive ||
      this.polling ||
      this.pending ||
      this.accepting ||
      this.id === null
    )
      return;
    if (this.current() && this.view && !this.running) return;
    const epoch = this.epoch;
    this.polling = true;
    try {
      await this.read(epoch);
    } catch (error) {
      if (this.active(epoch)) this.error = String(error);
    } finally {
      this.polling = false;
    }
  }
  async page(cueOffset: number, sectionOffset: number) {
    if (!this.canStart || !this.view || this.view.phase !== 'reviewing') return;
    this.pending = true;
    this.cueOffset = cueOffset;
    this.sectionOffset = sectionOffset;
    const epoch = ++this.epoch;
    try {
      await this.read(epoch);
    } catch (error) {
      if (this.active(epoch)) this.error = String(error);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  async cancel() {
    if (!this.alive || this.pending || this.id === null || this.cancelling)
      return;
    const epoch = ++this.epoch;
    this.cancelling = true;
    this.error = '';
    try {
      await this.api.resyncCancel(this.id);
      if (!this.accepting) await this.read(epoch);
    } catch (error) {
      if (this.active(epoch)) {
        this.cancelling = false;
        this.error = String(error);
      }
    }
  }
  async accept(): Promise<boolean> {
    if (!this.canAccept || this.id === null || !this.capture) return false;
    ++this.epoch; // Ignore any review response already in flight.
    this.accepting = true;
    this.error = '';
    try {
      const accepted = await this.subtitles.acceptResync(
        this.id,
        this.capture.document,
      );
      // A cancellation that arrives after Rust commits cannot roll it back.
      // The document owner still adopts that acknowledgment while the app lives.
      return this.alive && accepted;
    } catch (error) {
      if (this.alive) this.error = String(error);
      return false;
    } finally {
      if (this.alive) {
        this.accepting = false;
        this.cancelling = false;
      }
    }
  }
  dispose() {
    if (!this.alive) return;
    this.alive = false;
    ++this.epoch;
    if (this.id !== null) void this.api.resyncCancel(this.id).catch(() => {});
  }
}
