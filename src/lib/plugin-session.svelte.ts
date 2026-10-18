import {
  chooseFile,
  pluginStart,
  pluginView,
  pluginText,
  pluginCancel,
  type PluginInput,
  type PluginReview,
  type PluginTextChunk,
  type DocumentSnapshot,
} from './client';
import type { DocumentSession } from './document-session.svelte';

const client = {
  chooseFile,
  pluginStart,
  pluginView,
  pluginText,
  pluginCancel,
};
type Context = { session: number; quality: string; cursorMs: number | null };
type Setting = {
  key: string;
  type: 'text' | 'number' | 'boolean';
  value: string;
};
type Capture = {
  document: DocumentSnapshot;
  session: number;
  quality: string;
  path: string;
  input: string;
};

// This owner retains form/review projections only. The private proposal and Undo
// transaction stay in Rust; DocumentSession alone adopts accepted snapshots.
export class PluginSession {
  path = $state('');
  scope = $state<'all' | 'selected'>('all');
  includeCursor = $state(true);
  cps = $state('');
  rs = $state('');
  filter = $state('0');
  settings = $state<Setting[]>([]);
  mode = $state<PluginInput['mode']>('check');
  view = $state.raw<PluginReview | null>(null);
  detail = $state.raw<{
    row: number;
    after: boolean;
    chunk: PluginTextChunk;
  } | null>(null);
  pending = $state(false);
  accepting = $state(false);
  choosing = $state(false);
  applyingDraft = $state(false);
  uncertain = $state(false);
  error = $state('');
  private alive = true;
  private epoch = 0;
  private polling = false;
  private id: number | null = null;
  private capture: Capture | null = null;
  constructor(
    private subtitles: DocumentSession,
    private context: () => Context,
    private api = client,
  ) {}
  private input(): PluginInput {
    const uint = (value: string, name: string) => {
      const n = Number(value);
      if (!/^\d+$/.test(value) || !Number.isSafeInteger(n) || n > 0xffff_ffff)
        throw new Error(
          `${name} must be a whole number between 0 and 4294967295.`,
        );
      return n;
    };
    const cps = this.cps.trim() === '' ? null : Number(this.cps);
    if (cps !== null && (!Number.isFinite(cps) || cps <= 0))
      throw new Error(
        'CPS target must be a positive number, or blank to use the profile.',
      );
    const settings: PluginInput['settings'] = {};
    if (this.settings.length > 64)
      throw new Error('A plugin supports at most 64 settings.');
    for (const item of this.settings) {
      if (
        !/^[A-Za-z0-9_]{1,128}$/.test(item.key) ||
        ['constructor', 'prototype', '__proto__'].includes(item.key)
      )
        throw new Error(
          'Setting names need 1–128 letters, numbers or underscores; prototype names are reserved.',
        );
      if (Object.hasOwn(settings, item.key))
        throw new Error(`Duplicate setting: ${item.key}`);
      if (item.type === 'number') {
        if (
          item.value.trim() === '' ||
          !Number.isFinite(Number(item.value)) ||
          Math.abs(Number(item.value)) > Number.MAX_SAFE_INTEGER
        )
          throw new Error(`Setting ${item.key} needs a finite number.`);
        settings[item.key] = Number(item.value);
      } else if (item.type === 'boolean')
        settings[item.key] = item.value === 'true';
      else {
        if (new TextEncoder().encode(item.value).length > 4096)
          throw new Error(`Setting ${item.key} exceeds 4096 UTF-8 bytes.`);
        settings[item.key] = item.value;
      }
    }
    const selection = [...this.subtitles.selectedIds].map(String);
    if (this.scope === 'selected' && !selection.length)
      throw new Error('Select at least one cue.');
    return {
      mode: this.mode,
      scope: this.scope === 'selected' ? selection : [],
      selection,
      cursorMs: this.includeCursor ? this.context().cursorMs : null,
      settings,
      cpsTarget: cps,
      rsTargetIndex:
        this.rs.trim() === '' ? null : uint(this.rs, 'RS target index'),
      sceneFilterOffsetMs: uint(this.filter, 'Scene filter offset'),
    };
  }
  get validation() {
    try {
      this.input();
      return '';
    } catch (e) {
      return String(e);
    }
  }
  get current() {
    if (!this.capture || this.subtitles.dirty || this.validation) return false;
    const ctx = this.context();
    return (
      this.capture.document === this.subtitles.snapshot &&
      this.capture.path === this.path &&
      this.capture.session === ctx.session &&
      this.capture.quality === ctx.quality &&
      this.capture.input === JSON.stringify(this.input())
    );
  }
  get running() {
    return (
      this.id !== null &&
      (!this.view || ['preparing', 'cancelling'].includes(this.view.phase))
    );
  }
  get canStart() {
    return (
      this.alive &&
      !this.pending &&
      !this.accepting &&
      !this.running &&
      !this.uncertain
    );
  }
  get canRun() {
    return (
      this.canStart && !!this.path && !this.subtitles.dirty && !this.validation
    );
  }
  get canAccept() {
    return (
      this.canRun &&
      this.current &&
      this.view?.phase === 'reviewing' &&
      this.view.mode === 'fix' &&
      this.view.changesTotal > 0
    );
  }
  get canClose() {
    return (
      !this.accepting &&
      !this.choosing &&
      !this.applyingDraft &&
      !this.uncertain
    );
  }
  private active(epoch: number) {
    return this.alive && epoch === this.epoch;
  }
  private retire() {
    if (this.id !== null) void this.api.pluginCancel(this.id).catch(() => {});
    this.id = null;
    this.capture = null;
    this.view = null;
    this.detail = null;
  }
  private fail(error: unknown) {
    this.retire();
    this.error = String(error);
  }
  async pick() {
    if (!this.canStart) return;
    this.pending = this.choosing = true;
    const epoch = ++this.epoch;
    this.error = '';
    try {
      const path = await this.api.chooseFile('plugin', this.path || null);
      if (!this.active(epoch) || path === null) return;
      this.retire();
      this.path = path;
    } catch (e) {
      if (this.active(epoch)) this.error = String(e);
    } finally {
      if (this.active(epoch)) this.pending = this.choosing = false;
    }
  }
  async applyDraft() {
    if (!this.canStart) return;
    this.pending = this.applyingDraft = true;
    this.retire();
    try {
      await this.subtitles.apply();
    } catch (e) {
      if (this.alive) this.error = String(e);
    } finally {
      if (this.alive) this.pending = this.applyingDraft = false;
    }
  }
  useSettings() {
    if (!this.canStart || !this.view) return;
    this.settings = Object.entries(this.view.settings).map(([key, value]) => ({
      key,
      type:
        typeof value === 'number'
          ? 'number'
          : typeof value === 'boolean'
            ? 'boolean'
            : 'text',
      value: String(value),
    }));
  }
  async start(mode: PluginInput['mode']) {
    if (!this.canRun) return;
    this.mode = mode;
    const input = this.input();
    const ctx = this.context();
    const capture: Capture = {
      document: this.subtitles.snapshot,
      session: ctx.session,
      quality: ctx.quality,
      path: this.path,
      input: JSON.stringify(input),
    };
    const epoch = ++this.epoch;
    this.pending = true;
    this.error = '';
    this.view = null;
    this.detail = null;
    try {
      if (this.id !== null) await this.api.pluginCancel(this.id);
      if (!this.active(epoch)) return;
      this.id = null;
      this.capture = capture;
      if (!this.current)
        throw new Error('Subtitles or plugin context changed; run again.');
      const id = await this.api.pluginStart(
        capture.session,
        capture.document,
        capture.path,
        input,
      );
      if (!this.active(epoch)) {
        await this.api.pluginCancel(id).catch(() => {});
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
  private async read(
    epoch: number,
    findings = this.view?.findingsOffset ?? 0,
    changes = this.view?.changesOffset ?? 0,
  ) {
    if (this.id === null) return;
    const view = await this.api.pluginView(this.id, findings, changes, 50);
    if (!this.active(epoch)) return;
    if (!this.current) {
      this.fail('Subtitles, selection or plugin context changed; run again.');
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
    const epoch = this.epoch;
    if (!this.current) {
      this.fail('Subtitles, selection or plugin context changed; run again.');
      return;
    }
    if (
      this.view &&
      !['preparing', 'cancelling', 'reviewing'].includes(this.view.phase)
    )
      return;
    this.polling = true;
    try {
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) this.fail(e);
    } finally {
      this.polling = false;
    }
  }
  async page(findings: number, changes: number) {
    if (!this.canStart || !this.current) return;
    const epoch = ++this.epoch;
    this.pending = true;
    this.detail = null;
    try {
      await this.read(epoch, findings, changes);
    } catch (e) {
      if (this.active(epoch)) this.fail(e);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  async text(row: number, after: boolean, offset = 0) {
    if (!this.canStart || !this.current || this.id === null) return;
    const epoch = ++this.epoch;
    this.pending = true;
    try {
      const chunk = await this.api.pluginText(
        this.id,
        row,
        after,
        offset,
        16384,
      );
      if (!this.active(epoch)) return;
      if (!this.current) throw new Error('Plugin review changed; run again.');
      this.detail = { row, after, chunk };
    } catch (e) {
      if (this.active(epoch)) this.fail(e);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  async cancel() {
    if (!this.alive || this.pending || this.accepting || this.id === null)
      return;
    const epoch = ++this.epoch;
    this.pending = true;
    try {
      await this.api.pluginCancel(this.id);
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) this.fail(e);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  async reconcile() {
    if (!this.alive || this.pending || this.accepting || !this.uncertain)
      return;
    this.pending = true;
    try {
      await this.subtitles.command({ kind: 'status' }, { preserveDraft: true });
      if (this.alive) this.uncertain = false;
    } catch (e) {
      if (this.alive)
        this.error = `Could not refresh accepted subtitles: ${String(e)}`;
    } finally {
      if (this.alive) this.pending = false;
    }
  }
  async accept(): Promise<boolean> {
    if (!this.canAccept || this.id === null || !this.capture) return false;
    ++this.epoch;
    this.accepting = true;
    this.error = '';
    try {
      const accepted = await this.subtitles.acceptPlugin(
        this.id,
        this.capture.document,
        this.input(),
      );
      if (this.alive) this.retire();
      return this.alive && accepted;
    } catch (e) {
      if (this.alive) {
        this.fail(e);
        // The request may have committed in Rust even when its reply was lost.
        // Keep this modal open until authoritative state is available again.
        this.uncertain = true;
        try {
          await this.subtitles.command(
            { kind: 'status' },
            { preserveDraft: true },
          );
          if (this.alive) this.uncertain = false;
        } catch {
          /* Explicit retry remains available in the dialog. */
        }
      }
      return false;
    } finally {
      if (this.alive) this.accepting = false;
    }
  }
  dispose() {
    if (!this.alive) return;
    this.alive = false;
    ++this.epoch;
    this.retire();
  }
}
