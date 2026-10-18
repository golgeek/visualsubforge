import {
  styleForm,
  styleStart,
  styleView,
  styleCancel,
  type StyleForm,
  type StyleReview,
  type StyleOperation,
  type DocumentSnapshot,
} from './client';
import type { DocumentSession } from './document-session.svelte';
const client = { styleForm, styleStart, styleView, styleCancel };

// Only form drafts and review identity live here. Rust retains the candidate;
// DocumentSession is the sole frontend adoption path for accepted snapshots.
export class StyleSession {
  mode = $state<'edit' | 'new' | 'copy' | null>(null);
  fields = $state<StyleForm['fields']>([]);
  view = $state.raw<StyleReview | null>(null);
  pending = $state(false);
  accepting = $state(false);
  applyingDraft = $state(false);
  error = $state('');
  private alive = true;
  private epoch = 0;
  private polling = false;
  private id: number | null = null;
  private selected = '';
  private baseline: StyleForm['fields'] = [];
  private formDocument: DocumentSnapshot | null = null;
  private capture: { document: DocumentSnapshot; operation: string } | null =
    null;
  constructor(
    private subtitles: DocumentSession,
    private api = client,
  ) {}
  get operation(): StyleOperation {
    const fields = this.fields
      .filter((f, i) => f.editable && f.value !== this.baseline[i]?.value)
      .map(({ name, value }) => ({ name, value }));
    if (this.mode === 'edit')
      return { kind: 'edit', name: this.selected, fields };
    return {
      kind: 'create',
      name:
        this.fields.find((f) => f.name.toLowerCase() === 'name')?.value ?? '',
      copyFrom: this.mode === 'copy' ? this.selected : null,
      fields: fields.filter((f) => f.name.toLowerCase() !== 'name'),
    };
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
      this.capture.operation === JSON.stringify(this.operation)
    );
  }
  get formCurrent() {
    return this.formDocument === this.subtitles.snapshot;
  }
  get canAccept() {
    return (
      this.canStart &&
      this.current &&
      this.view?.phase === 'reviewing' &&
      this.view.changed
    );
  }
  private active(epoch: number) {
    return this.alive && epoch === this.epoch;
  }
  async applyDraft() {
    if (!this.canStart) return;
    this.pending = this.applyingDraft = true;
    try {
      await this.subtitles.apply();
    } catch (e) {
      if (this.alive) this.error = String(e);
    } finally {
      if (this.alive) this.pending = this.applyingDraft = false;
    }
  }
  async begin(mode: 'edit' | 'new' | 'copy', selected: string) {
    if (!this.canStart || this.subtitles.dirty) return;
    const epoch = ++this.epoch;
    const document = this.subtitles.snapshot;
    this.pending = true;
    this.error = '';
    this.view = null;
    try {
      if (this.id !== null) await this.api.styleCancel(this.id);
      if (!this.active(epoch)) return;
      this.id = null;
      this.capture = null;
      const form = await this.api.styleForm(
        document,
        mode === 'new' ? null : selected,
      );
      if (!this.active(epoch)) return;
      if (document !== this.subtitles.snapshot || this.subtitles.dirty)
        throw new Error('Subtitles changed; open the style form again.');
      this.mode = mode;
      this.selected = selected;
      this.formDocument = document;
      this.baseline = structuredClone(form.fields);
      this.fields = structuredClone(form.fields);
      if (mode !== 'edit') {
        const name = this.fields.find((f) => f.name.toLowerCase() === 'name');
        if (name)
          name.value = mode === 'copy' ? `${selected} copy` : 'New style';
      }
    } catch (e) {
      if (this.active(epoch)) {
        this.error = String(e);
        this.mode = null;
      }
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  async start() {
    if (
      !this.canStart ||
      !this.mode ||
      this.subtitles.dirty ||
      !this.formCurrent
    )
      return;
    const operation = this.operation;
    const capture = {
      document: this.subtitles.snapshot,
      operation: JSON.stringify(operation),
    };
    const epoch = ++this.epoch;
    this.pending = true;
    this.error = '';
    this.view = null;
    try {
      if (this.id !== null) await this.api.styleCancel(this.id);
      if (!this.active(epoch)) return;
      this.id = null;
      this.capture = capture;
      const id = await this.api.styleStart(capture.document, operation);
      if (!this.active(epoch)) {
        await this.api.styleCancel(id).catch(() => {});
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
    if (this.id !== null) void this.api.styleCancel(this.id).catch(() => {});
    this.id = null;
    this.capture = null;
    this.view = null;
    this.error = String(error);
  }
  private async read(epoch: number) {
    if (this.id === null) return;
    const view = await this.api.styleView(this.id);
    if (!this.active(epoch)) return;
    if (!this.current) {
      this.fail('Style form or subtitles changed; preview again.');
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
    this.polling = true;
    const epoch = this.epoch;
    try {
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) this.fail(e);
    } finally {
      this.polling = false;
    }
  }
  async cancel() {
    if (!this.alive || this.id === null || this.pending || this.accepting)
      return;
    const epoch = ++this.epoch;
    this.pending = true;
    try {
      await this.api.styleCancel(this.id);
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) this.fail(e);
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
      const accepted = await this.subtitles.acceptStyle(
        this.id,
        this.capture.document,
      );
      return this.alive && accepted;
    } catch (e) {
      if (this.alive) this.error = String(e);
      return false;
    } finally {
      if (this.alive) this.accepting = false;
    }
  }
  discardForm() {
    if (!this.canStart) return;
    ++this.epoch;
    if (this.id !== null) void this.api.styleCancel(this.id).catch(() => {});
    this.id = null;
    this.capture = null;
    this.formDocument = null;
    this.mode = null;
    this.fields = [];
    this.view = null;
    this.error = '';
  }
  dispose() {
    if (!this.alive) return;
    this.alive = false;
    ++this.epoch;
    if (this.id !== null) void this.api.styleCancel(this.id).catch(() => {});
  }
}
