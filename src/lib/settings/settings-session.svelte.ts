import { tick } from 'svelte';
import {
  getPreferences,
  getQualitySettings,
  getLanguageWords,
  savePreferences,
  type Preferences,
  type QualityProfile,
} from '../client';
import { shortcutError } from '../workspace/shortcut-settings';
const api = {
  getPreferences,
  getQualitySettings,
  getLanguageWords,
  savePreferences,
};
const copy = (value: Preferences): Preferences =>
  structuredClone($state.snapshot(value));
const identity = (value: Preferences) =>
  JSON.stringify({ ...value, layout: null }, (_, item) =>
    item && typeof item === 'object' && !Array.isArray(item)
      ? Object.fromEntries(
          Object.entries(item).sort(([a], [b]) => a.localeCompare(b)),
        )
      : item,
  );
export const SETTINGS_DELAY_MS = 250;
/** Dialog draft and confirmed preference checkpoints only. Rust owns policy,
 * persistence and epochs. A failed mutation is reconciled read-only, never replayed. */
export class SettingsSession {
  draft: Preferences = $state(null!);
  builtins = $state.raw<QualityProfile[]>([]);
  ready = $state(false);
  saving = $state(false);
  loading = $state(false);
  error = $state('');
  notice = $state('');
  validation = $state('');
  uncertain = $state(false);
  private confirmed = $state.raw<Preferences>(null!);
  private undoValue = $state.raw<Preferences | null>(null);
  private epoch: number | null = null;
  private alive = true;
  private sequence = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private pending: Promise<boolean> | null = null;
  private failedAttempt: Preferences | null = null;
  private failedRemember = false;
  private retryRequired = false;
  constructor(
    initial: Preferences,
    private accepted: (value: Preferences) => void,
    private valid: () => string | null,
    private client = api,
  ) {
    this.draft = copy(initial);
    this.confirmed = copy(initial);
  }
  async load() {
    const sequence = ++this.sequence;
    this.loading = true;
    this.error = '';
    try {
      const [data, words] = await Promise.all([
        this.client.getQualitySettings(),
        this.client.getLanguageWords(),
      ]);
      if (!this.alive || sequence !== this.sequence) return;
      if (data.current.epoch !== words.epoch)
        throw new Error(
          'Settings changed while opening; close and reopen Settings.',
        );
      this.draft = copy({
        ...this.confirmed,
        quality: data.current.profile,
        timing: data.current.timing,
        personalWords: words.personal,
      });
      this.confirmed = copy(this.draft);
      this.epoch = data.current.epoch;
      this.builtins = data.builtins;
      this.ready = true;
    } catch (error) {
      if (this.alive) this.error = String(error);
    } finally {
      if (this.alive) this.loading = false;
    }
  }
  get dirty() {
    return identity(this.draft) !== identity(this.confirmed);
  }
  get canUndo() {
    return !!this.undoValue && !this.saving && !this.loading && !this.uncertain;
  }
  private clearTimer() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
  async changed() {
    this.clearTimer();
    await tick();
    if (!this.alive || !this.ready || this.uncertain) return;
    // A field handler and the bubbling form event can both resume after tick.
    // Retire the previous continuation's timer before replacing its handle.
    this.clearTimer();
    this.validation = this.valid() ?? shortcutError(this.draft.shortcuts) ?? '';
    if (this.validation || !this.dirty || this.retryRequired) return;
    this.error = '';
    this.notice = '';
    if (this.saving) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      if (!this.retryRequired && !this.uncertain) void this.flush();
    }, SETTINGS_DELAY_MS);
  }
  async flush(): Promise<boolean> {
    this.clearTimer();
    if (this.pending) {
      if (!(await this.pending)) return false;
    }
    if (!this.alive || !this.ready || this.uncertain) return false;
    await tick();
    this.validation = this.valid() ?? shortcutError(this.draft.shortcuts) ?? '';
    if (this.validation) return false;
    if (!this.dirty) return true;
    return this.save(copy(this.draft), true);
  }
  private save(value: Preferences, remember: boolean): Promise<boolean> {
    if (!this.alive || this.epoch === null || this.pending)
      return Promise.resolve(false);
    this.saving = true;
    this.error = '';
    const before = copy(this.confirmed);
    const pending = (async () => {
      await Promise.resolve();
      try {
        if (!this.alive) return false;
        const epoch = await this.client.savePreferences(value, this.epoch!);
        if (!this.alive) return false;
        if (!Number.isSafeInteger(epoch) || epoch <= this.epoch!)
          throw new Error(
            'Missing confirmed settings identity. Check saved settings before retrying.',
          );
        this.epoch = epoch;
        this.confirmed = copy(value);
        this.retryRequired = false;
        this.undoValue = remember ? before : null;
        this.notice = remember ? 'Settings saved' : 'Settings change undone';
        this.accepted(copy(value));
        return true;
      } catch (error) {
        if (this.alive) {
          this.error = String(error);
          this.uncertain = true;
          this.retryRequired = true;
          this.failedAttempt = value;
          this.failedRemember = remember;
          this.clearTimer();
        }
        return false;
      } finally {
        this.pending = null;
        if (this.alive) {
          this.saving = false;
          if (!this.uncertain && this.dirty) void this.changed();
        }
      }
    })();
    this.pending = pending;
    return pending;
  }
  async undo() {
    if (!this.canUndo || !this.undoValue) return;
    this.clearTimer();
    const previous = copy(this.undoValue);
    this.draft = copy(previous);
    this.validation = '';
    await this.save(previous, false);
  }
  async reconcile() {
    if (!this.alive || this.saving || this.loading || !this.uncertain) return;
    this.clearTimer();
    this.loading = true;
    try {
      const saved = await this.client.getPreferences();
      const [data, words] = await Promise.all([
        this.client.getQualitySettings(),
        this.client.getLanguageWords(),
      ]);
      if (!this.alive) return;
      if (data.current.epoch !== words.epoch)
        throw new Error('Settings changed during refresh; check again.');
      const current = copy({
        ...saved,
        quality: data.current.profile,
        timing: data.current.timing,
        personalWords: words.personal,
      });
      const wasAccepted =
        this.failedAttempt &&
        identity(current) === identity(this.failedAttempt);
      if (wasAccepted)
        this.undoValue = this.failedRemember ? copy(this.confirmed) : null;
      else if (identity(current) !== identity(this.confirmed))
        this.undoValue = null;
      this.confirmed = current;
      this.retryRequired = this.dirty;
      this.epoch = data.current.epoch;
      this.uncertain = false;
      this.failedAttempt = null;
      this.accepted(copy(current));
      this.error = '';
      this.notice = this.dirty
        ? 'Saved settings checked. Your draft is retained; review it, then retry.'
        : wasAccepted
          ? 'The settings change was saved. No retry was sent.'
          : 'Saved settings checked. Your draft matches the saved settings.';
      // Deliberately do not schedule a retry from this read-only recovery.
    } catch (error) {
      if (this.alive) this.error = String(error);
    } finally {
      if (this.alive) this.loading = false;
    }
  }
  dispose() {
    this.alive = false;
    ++this.sequence;
    this.clearTimer();
  }
}
