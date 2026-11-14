import { defaultPreferences, type Preferences } from './client';
import { defaultTiming, type TimingSettings } from './magnetic-timing';
/** Saved defaults, effective project timing and quality invalidation share one owner. */
export class WorkspacePreferences {
  private saved = $state<Preferences>({
    ...defaultPreferences,
    timing: { ...defaultTiming },
  });
  private timing = $state({ ...defaultTiming });
  private context = $state(0);
  showQualityMetrics = $state(false);
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
  accept(value: Preferences) {
    this.initialize(value);
    this.invalidateQuality();
  }
  restoreTiming(value?: TimingSettings | null) {
    this.invalidateQuality();
    this.timing = { ...(value ?? this.saved.timing) };
  }
  resetTiming() {
    this.timing = { ...this.saved.timing };
  }
  invalidateQuality() {
    ++this.context;
  }
}
