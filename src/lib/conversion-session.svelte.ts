import {
  chooseFile,
  conversionStart,
  conversionView,
  conversionCancel,
  conversionPublish,
  type ConversionOptions,
  type ConversionReview,
  type DocumentSnapshot,
  type FileChoice,
} from './client';
import type { DocumentSession } from './document-session.svelte';
const client = {
  chooseFile,
  conversionStart,
  conversionView,
  conversionCancel,
  conversionPublish,
};

// Only options, bounded report projection and request lifetime live here. Export
// does not replace DocumentSession's snapshot or change accepted/saved history.
export class ConversionSession {
  options = $state.raw<ConversionOptions>({
    target: 'ass',
    precision: 'exact',
  });
  view = $state.raw<ConversionReview | null>(null);
  pending = $state(false);
  picking = $state(false);
  publishing = $state(false);
  applyingDraft = $state(false);
  error = $state('');
  private alive = true;
  private epoch = 0;
  private polling = false;
  private id: number | null = null;
  private capture: { document: DocumentSnapshot; options: string } | null =
    null;
  constructor(
    private subtitles: DocumentSession,
    private api = client,
  ) {
    this.options = {
      target: this.source === 'srt' ? 'ass' : 'srt',
      precision: 'exact',
    };
  }
  get source() {
    return (this.subtitles.snapshot.format?.name ?? 'srt').toLowerCase();
  }
  get running() {
    return (
      this.id !== null &&
      (this.publishing ||
        !this.view ||
        ['preparing', 'publishing', 'cancelling'].includes(this.view.phase))
    );
  }
  get canClose() {
    return !this.picking && !this.publishing && !this.applyingDraft;
  }
  get canConfigure() {
    return this.alive && !this.pending && !this.running && !this.publishing;
  }
  get canStart() {
    return (
      this.canConfigure &&
      this.subtitles.snapshot.loaded &&
      !this.subtitles.dirty &&
      this.options.target !== this.source
    );
  }
  get current() {
    return (
      this.capture?.document === this.subtitles.snapshot &&
      !this.subtitles.dirty &&
      this.capture?.options === JSON.stringify(this.options)
    );
  }
  get canPublish() {
    return (
      this.canStart &&
      this.current &&
      !!this.view &&
      ['reviewing', 'failed'].includes(this.view.phase) &&
      this.view.report?.outputBytes != null
    );
  }
  private active(epoch: number) {
    return this.alive && epoch === this.epoch;
  }
  configure(options: ConversionOptions) {
    if (!this.canConfigure) return;
    this.options = {
      ...options,
      precision: options.target === 'srt' ? 'exact' : options.precision,
    };
    ++this.epoch;
    if (this.id !== null)
      void this.api.conversionCancel(this.id).catch(() => {});
    this.id = null;
    this.capture = null;
    this.view = null;
    this.error = '';
  }
  async applyDraft() {
    if (!this.canConfigure) return;
    this.pending = this.applyingDraft = true;
    try {
      await this.subtitles.apply();
    } catch (e) {
      if (this.alive) this.error = String(e);
    } finally {
      if (this.alive) this.pending = this.applyingDraft = false;
    }
  }
  async start() {
    if (!this.canStart) return;
    const capture = {
      document: this.subtitles.snapshot,
      options: JSON.stringify(this.options),
    };
    const options = { ...this.options };
    const epoch = ++this.epoch;
    this.pending = true;
    this.error = '';
    this.view = null;
    try {
      if (this.id !== null) await this.api.conversionCancel(this.id);
      if (!this.active(epoch)) return;
      this.id = null;
      this.capture = capture;
      const id = await this.api.conversionStart(capture.document, options);
      if (!this.active(epoch)) {
        await this.api.conversionCancel(id).catch(() => {});
        return;
      }
      this.id = id;
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) this.fail(e);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  private fail(error: unknown) {
    if (this.id !== null)
      void this.api.conversionCancel(this.id).catch(() => {});
    this.id = null;
    this.capture = null;
    this.view = null;
    this.error = String(error);
  }
  private async read(epoch: number) {
    if (this.id === null) return;
    const view = await this.api.conversionView(this.id);
    if (!this.active(epoch)) return;
    if (view.phase === 'published') {
      // Publication can win a late cancel/edit race. Always retain its receipt.
      this.view = view;
      this.publishing = false;
      this.error = '';
      return;
    }
    if (this.publishing && ['publishing', 'cancelling'].includes(view.phase)) {
      this.view = view;
      if (!this.current) {
        this.error =
          'Subtitles changed during export; waiting for its final status.';
        await this.api.conversionCancel(this.id);
      }
      return;
    }
    this.publishing = false;
    if (!this.current) {
      this.fail(
        'Subtitles, the cue draft or conversion options changed; review again.',
      );
      return;
    }
    this.view = view;
    this.error = view.error ?? '';
  }
  async poll() {
    if (!this.alive || this.pending || this.polling || this.id === null) return;
    this.polling = true;
    const epoch = this.epoch;
    try {
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) {
        if (this.publishing)
          this.error = `Export status is unavailable; checking again. ${String(e)}`;
        else this.fail(e);
      }
    } finally {
      this.polling = false;
    }
  }
  async publish() {
    if (!this.canPublish || this.id === null || !this.capture) return;
    const epoch = ++this.epoch;
    this.pending = this.picking = true;
    this.error = '';
    try {
      const kind: Record<ConversionOptions['target'], FileChoice> = {
        srt: 'exportSrt',
        ass: 'exportAss',
        ssa: 'exportSsa',
      };
      const original = this.capture.document.path ?? 'Untitled';
      const dot = original.lastIndexOf('.');
      const separator = Math.max(
        original.lastIndexOf('/'),
        original.lastIndexOf('\\'),
      );
      const stem = dot > separator + 1 ? original.slice(0, dot) : original;
      const path = await this.api.chooseFile(
        kind[this.options.target],
        `${stem}.converted.${this.options.target}`,
      );
      if (!this.active(epoch)) return;
      if (!this.current)
        throw new Error(
          'Subtitles changed while choosing an output file; review again.',
        );
      if (!path) return;
      this.picking = false;
      this.publishing = true;
      let requestError = '';
      try {
        await this.api.conversionPublish(this.id, path);
      } catch (e) {
        requestError = String(e);
      }
      await this.read(epoch);
      if (
        this.active(epoch) &&
        requestError &&
        this.view?.phase !== 'published'
      )
        this.error = requestError;
    } catch (e) {
      if (this.active(epoch))
        this.error = this.publishing
          ? `Export status is unavailable; checking again. ${String(e)}`
          : String(e);
    } finally {
      if (this.active(epoch)) this.pending = this.picking = false;
    }
  }
  async cancel() {
    if (!this.alive || this.id === null || this.pending) return;
    const epoch = ++this.epoch;
    this.pending = true;
    try {
      await this.api.conversionCancel(this.id);
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) this.error = String(e);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  dispose() {
    if (!this.alive) return;
    this.alive = false;
    ++this.epoch;
    if (this.id !== null)
      void this.api.conversionCancel(this.id).catch(() => {});
  }
}
