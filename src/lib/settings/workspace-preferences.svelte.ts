import {
  saveTimingRepairOptions,
  type TimingFixOptions,
  defaultPreferences,
  type Preferences,
} from '../client';
import { defaultTiming, type TimingSettings } from '../timing/magnetic-timing';
/** Saved defaults, effective project timing and quality invalidation share one owner. */
export class WorkspacePreferences {
  private saved = $state<Preferences>({
    ...defaultPreferences,
    timing: { ...defaultTiming },
  });
  private timing = $state({ ...defaultTiming });
  private context = $state(0);
  showQualityMetrics = $state(defaultPreferences.showQualityMetrics);
  get preferences() {
    return this.saved;
  }
  get timingSettings() {
    return this.timing;
  }
  get qualityContext() {
    return this.context;
  }
  initialize(value: Preferences) {
    this.saved = { ...value, timing: { ...value.timing } };
    this.timing = { ...value.timing };
    this.showQualityMetrics = value.showQualityMetrics;
  }
  private repairWrite: Promise<unknown> = Promise.resolve();
  async saveRepair(options: TimingFixOptions) {
    this.saved = { ...this.saved, timingRepair: options };
    const confirmed = { ...options };
    const write = this.repairWrite.then(() =>
      saveTimingRepairOptions(confirmed),
    );
    this.repairWrite = write.catch(() => {});
    await write;
  }
  acceptUpdateConsent(enabled: boolean) {
    this.saved = { ...this.saved, automaticUpdates: enabled };
  }
  acceptLayout(layout: Preferences['layout']) {
    this.saved = { ...this.saved, layout };
  }
  accept(value: Preferences) {
    this.initialize({
      ...value,
      layout: this.saved.layout,
      automaticUpdates: this.saved.automaticUpdates,
      timingRepair: this.saved.timingRepair,
    });
    this.invalidateQuality();
  }
  restoreTiming(value?: TimingSettings | null) {
    this.invalidateQuality();
    this.timing = { ...(value ?? this.saved.timing) };
  }
  setSnapping(enabled: boolean) {
    this.timing = { ...this.timing, enabled };
  }
  resetTiming() {
    this.timing = { ...this.saved.timing };
  }
  invalidateQuality() {
    ++this.context;
  }
}
