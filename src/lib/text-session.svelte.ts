import {
  textStart,
  textView,
  textCancel,
  type TextView,
  type DocumentSnapshot,
  type TextOperation,
  type TextTarget,
  textMatches,
} from './client';
import type { DocumentSession } from './document-session.svelte';
const client = { textStart, textView, textCancel, textMatches };

// Dialog lifetime and bounded projections only. Rust owns the immutable proposal;
// DocumentSession reconciles its accepted reply with ordinary document operations.
export class TextSession {
  view = $state.raw<TextView | null>(null);
  error = $state('');
  pending = $state(false);
  accepting = $state(false);
  applyingDraft = $state(false);
  mode = $state<TextOperation['kind']>('find');
  query = $state('');
  replacement = $state('');
  caseSensitive = $state(false);
  languageTarget = $state<{
    cueIndex: number;
    start: number;
    end: number;
    original: string;
  }>({ cueIndex: 0, start: 0, end: 0, original: '' });
  get operation(): TextOperation {
    if (this.mode === 'languageSuggestions')
      return { kind: this.mode, ...this.languageTarget };
    if (this.mode === 'languageCorrection')
      return {
        kind: this.mode,
        ...this.languageTarget,
        replacement: this.replacement,
      };
    if (this.mode === 'find')
      return {
        kind: 'find',
        query: this.query,
        caseSensitive: this.caseSensitive,
      };
    if (this.mode === 'replace')
      return {
        kind: 'replace',
        query: this.query,
        replacement: this.replacement,
        caseSensitive: this.caseSensitive,
      };
    return { kind: this.mode };
  }
  private alive = true;
  private epoch = 0;
  private id: number | null = null;
  private capture: { document: DocumentSnapshot; operation: string } | null =
    null;
  private polling = false;
  private offset = 0;
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
  get canAccept() {
    return (
      this.canStart &&
      this.current &&
      this.view?.phase === 'reviewing' &&
      this.view.changed > 0
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
      operation: JSON.stringify(this.operation),
    };
    const epoch = ++this.epoch;
    this.pending = true;
    this.error = '';
    this.view = null;
    this.offset = 0;
    try {
      if (this.id !== null) await this.api.textCancel(this.id);
      if (!this.active(epoch)) return;
      this.id = null;
      this.capture = capture;
      const id = await this.api.textStart(capture.document, this.operation);
      if (!this.active(epoch)) {
        await this.api.textCancel(id).catch(() => {});
        return;
      }
      this.id = id;
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) {
        if (this.id !== null) void this.api.textCancel(this.id).catch(() => {});
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
    const view = await this.api.textView(this.id, this.offset, 100);
    if (!this.active(epoch)) return;
    if (!this.current) {
      await this.api.textCancel(this.id).catch(() => {});
      if (!this.active(epoch)) return;
      this.view = null;
      this.id = null;
      this.capture = null;
      this.error = 'Workspace or cue draft changed; preview again.';
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
    // Poll completed proposals too: profile changes are Rust-owned.
    this.polling = true;
    const epoch = this.epoch;
    try {
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) {
        if (this.id !== null) void this.api.textCancel(this.id).catch(() => {});
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
      await this.api.textCancel(this.id);
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) this.error = String(e);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  async matches(cueIndex: number, offset: number) {
    if (!this.current || this.id === null || this.view?.phase !== 'reviewing')
      throw new Error('Search changed; search again.');
    const epoch = this.epoch;
    const found = await this.api.textMatches(this.id, cueIndex, offset, 100);
    if (!this.active(epoch) || !this.current)
      throw new Error('Search changed; search again.');
    return found;
  }
  async accept(target: TextTarget = null): Promise<boolean> {
    if (!this.canAccept || this.id === null || !this.capture) return false;
    ++this.epoch;
    this.accepting = true;
    this.error = '';
    try {
      const accepted = await this.subtitles.acceptText(
        this.id,
        this.capture.document,
        target,
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
    if (this.id !== null) void this.api.textCancel(this.id).catch(() => {});
  }
}
