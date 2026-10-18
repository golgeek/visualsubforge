import {
  chooseFile,
  fontStart,
  fontView,
  fontCancel,
  type FontReview,
  type FontOperation,
  type FontInventory,
  type DocumentSnapshot,
} from './client';
import type { DocumentSession } from './document-session.svelte';
const client = { chooseFile, fontStart, fontView, fontCancel };

// Metadata and review identity are transient. Payloads and accepted history stay
// in Rust; every accepted snapshot is reconciled by DocumentSession.
export class FontSession {
  inventory = $state.raw<FontInventory | null>(null);
  view = $state.raw<FontReview | null>(null);
  selected = $state<number | null>(null);
  pending = $state(false);
  accepting = $state(false);
  applyingDraft = $state(false);
  picking = $state(false);
  error = $state('');
  private alive = true;
  private epoch = 0;
  private polling = false;
  private id: number | null = null;
  private inventoryDocument: DocumentSnapshot | null = null;
  private capture: {
    document: DocumentSnapshot;
    operation: FontOperation;
  } | null = null;
  constructor(
    private subtitles: DocumentSession,
    private api = client,
  ) {}
  get running() {
    return (
      this.id !== null &&
      (!this.view || ['preparing', 'cancelling'].includes(this.view.phase))
    );
  }
  get canClose() {
    return !this.accepting && !this.applyingDraft && !this.picking;
  }
  get canStart() {
    return this.alive && !this.pending && !this.accepting && !this.running;
  }
  get current() {
    return (
      !!this.capture &&
      this.capture.document === this.subtitles.snapshot &&
      (this.capture.operation.kind === 'inspect' || !this.subtitles.dirty)
    );
  }
  get inventoryCurrent() {
    return this.inventoryDocument === this.subtitles.snapshot;
  }
  get entry() {
    return this.inventory?.entries.find((e) => e.id === this.selected);
  }
  get editable() {
    return (
      this.entry?.kind === 'font' && !!this.entry.font && !this.entry.problem
    );
  }
  get canChange() {
    return (
      this.canStart &&
      this.inventoryCurrent &&
      !!this.inventory?.complete &&
      !this.subtitles.dirty
    );
  }
  get canPut() {
    return (
      this.canChange &&
      !this.inventory?.entries.some((e) => e.kind === 'font' && e.problem)
    );
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
  async inspect() {
    if (this.canStart) await this.launch({ kind: 'inspect' });
  }
  async pick(replace = false) {
    if (!this.canPut || (replace && !this.editable)) return;
    const epoch = ++this.epoch;
    const document = this.subtitles.snapshot;
    const selected = this.selected;
    this.pending = this.picking = true;
    this.error = '';
    try {
      const path = await this.api.chooseFile('font');
      if (!this.active(epoch)) return;
      if (document !== this.subtitles.snapshot || this.subtitles.dirty)
        throw new Error(
          'Subtitles changed while choosing a font; inspect attachments again.',
        );
      this.pending = this.picking = false;
      if (path)
        await this.launch(
          replace && selected !== null
            ? { kind: 'replace', id: selected, path }
            : { kind: 'add', path },
        );
    } catch (e) {
      if (this.active(epoch)) this.error = String(e);
    } finally {
      if (this.active(epoch)) this.pending = this.picking = false;
    }
  }
  async remove() {
    if (this.canChange && this.editable && this.selected !== null)
      await this.launch({ kind: 'remove', id: this.selected });
  }
  private async launch(operation: FontOperation) {
    const capture = { document: this.subtitles.snapshot, operation };
    const epoch = ++this.epoch;
    this.pending = true;
    this.error = '';
    this.view = null;
    try {
      if (this.id !== null) await this.api.fontCancel(this.id);
      if (!this.active(epoch)) return;
      this.id = null;
      this.capture = capture;
      const id = await this.api.fontStart(capture.document, operation);
      if (!this.active(epoch)) {
        await this.api.fontCancel(id).catch(() => {});
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
    if (this.id !== null) void this.api.fontCancel(this.id).catch(() => {});
    this.id = null;
    this.capture = null;
    this.view = null;
    this.error = String(error);
  }
  private async read(epoch: number) {
    if (this.id === null) return;
    const view = await this.api.fontView(this.id);
    if (!this.active(epoch)) return;
    if (!this.current) {
      this.fail(
        'Subtitles or the cue draft changed; inspect attachments again.',
      );
      return;
    }
    this.view = view;
    if (view.error) this.error = view.error;
    if (view.phase === 'reviewing' && view.inventory) {
      this.inventory = view.inventory;
      this.inventoryDocument = this.capture!.document;
      if (!this.inventory.entries.some((e) => e.id === this.selected))
        this.selected = this.inventory.entries[0]?.id ?? null;
    }
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
      await this.api.fontCancel(this.id);
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
      const accepted = await this.subtitles.acceptFont(
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
  dispose() {
    if (!this.alive) return;
    this.alive = false;
    ++this.epoch;
    if (this.id !== null) void this.api.fontCancel(this.id).catch(() => {});
  }
}
