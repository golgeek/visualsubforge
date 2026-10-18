import {
  chooseFile,
  projectImportStart,
  projectImportView,
  projectImportCancel,
  projectImportPublish,
  type LegacyImportRequest,
  type LegacyImportReview,
} from './client';

const client = {
  chooseFile,
  projectImportStart,
  projectImportView,
  projectImportCancel,
  projectImportPublish,
};
type Input = 'path' | 'mediaPath' | 'subtitlePath';

// This owner has no document/media references. Inputs, report projection and
// response lifetime are transient; Rust owns the immutable review and export.
export class LegacyImportSession {
  request = $state.raw<LegacyImportRequest>({
    path: '',
    mediaPath: null,
    subtitlePath: null,
  });
  view = $state.raw<LegacyImportReview | null>(null);
  pending = $state(false);
  picking = $state(false);
  publishing = $state(false);
  error = $state('');
  private alive = true;
  private epoch = 0;
  private polling = false;
  private id: number | null = null;
  constructor(private api = client) {}
  get running() {
    return (
      this.publishing || (this.id !== null && (!this.view || this.view.running))
    );
  }
  get canClose() {
    return !this.picking && !this.publishing;
  }
  get canConfigure() {
    return this.alive && !this.pending && !this.running;
  }
  get canStart() {
    return this.canConfigure && !!this.request.path;
  }
  get canPublish() {
    return (
      this.canConfigure &&
      this.view?.phase === 'reviewing' &&
      !!this.view.report?.canPublish
    );
  }
  private active(epoch: number) {
    return this.alive && this.epoch === epoch;
  }
  private retire() {
    if (this.id !== null)
      void this.api.projectImportCancel(this.id).catch(() => {});
    this.id = null;
    this.view = null;
  }
  reset(field: Exclude<Input, 'path'>) {
    if (!this.canConfigure) return;
    ++this.epoch;
    this.retire();
    this.request = { ...this.request, [field]: null };
    this.error = '';
  }
  async pick(field: Input) {
    if (!this.canConfigure) return;
    const epoch = ++this.epoch;
    this.pending = this.picking = true;
    this.error = '';
    try {
      const reference =
        field === 'mediaPath'
          ? this.view?.report?.media
          : this.view?.report?.subtitles;
      const path = await this.api.chooseFile(
        field === 'path'
          ? 'legacyProject'
          : field === 'mediaPath'
            ? 'media'
            : 'subtitles',
        this.request[field] ||
          reference?.resolved ||
          reference?.candidate ||
          this.request.path ||
          null,
      );
      if (!this.active(epoch) || !path) return;
      this.retire();
      // Relocations belong to one source, never silently carry to a new project.
      this.request =
        field === 'path'
          ? { path, mediaPath: null, subtitlePath: null }
          : { ...this.request, [field]: path };
    } catch (e) {
      if (this.active(epoch)) this.error = String(e);
    } finally {
      if (this.active(epoch)) this.pending = this.picking = false;
    }
  }
  async start() {
    if (!this.canStart) return;
    const epoch = ++this.epoch;
    const request = { ...this.request };
    this.pending = true;
    this.error = '';
    try {
      if (this.id !== null) await this.api.projectImportCancel(this.id);
      if (!this.active(epoch)) return;
      this.id = null;
      this.view = null;
      const id = await this.api.projectImportStart(request);
      if (!this.active(epoch)) {
        await this.api.projectImportCancel(id).catch(() => {});
        return;
      }
      this.id = id;
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) {
        this.retire();
        this.error = String(e);
      }
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  private async read(epoch: number) {
    if (this.id === null) return;
    const view = await this.api.projectImportView(this.id);
    if (!this.active(epoch)) return;
    this.view = view;
    this.publishing =
      view.phase === 'publishing' ||
      (this.publishing && view.phase === 'cancelling');
    this.error = view.error ?? '';
  }
  async poll() {
    if (!this.alive || this.pending || this.polling || this.id === null) return;
    const epoch = this.epoch;
    this.polling = true;
    try {
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) {
        if (this.publishing)
          this.error = `Project creation status is unavailable; checking again. ${String(e)}`;
        else {
          this.retire();
          this.error = String(e);
        }
      }
    } finally {
      this.polling = false;
    }
  }
  async publish() {
    if (!this.canPublish || this.id === null) return;
    const epoch = ++this.epoch;
    this.pending = this.picking = true;
    this.error = '';
    try {
      const original = this.request.path;
      const dot = original.lastIndexOf('.');
      const separator = Math.max(
        original.lastIndexOf('/'),
        original.lastIndexOf('\\'),
      );
      const stem = dot > separator + 1 ? original.slice(0, dot) : original;
      const path = await this.api.chooseFile(
        'saveProject',
        `${stem}.imported.vsfproject`,
      );
      if (!this.active(epoch) || !path) return;
      this.picking = false;
      this.publishing = true;
      let requestError = '';
      try {
        await this.api.projectImportPublish(this.id, path);
      } catch (e) {
        requestError = String(e);
      }
      // An ACK or transport error is not a write receipt. Keep the dialog and
      // duplicate-export gate until an authoritative terminal status is known.
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
          ? `Project creation status is unavailable; checking again. ${String(e)}`
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
      await this.api.projectImportCancel(this.id);
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
      void this.api.projectImportCancel(this.id).catch(() => {});
  }
}
