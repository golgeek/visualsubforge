import type { QualityProfile, TimingSettings } from './client';

export const defaultLanguageTools: NonNullable<
  QualityProfile['languageTools']
> = {
  dictionary: 'en-US',
  spelling: false,
  typography: false,
  capitalization: false,
  dialogueMarkers: false,
};

// Draft transformations only. Rust validates and accepts the effective policy.
// JSON copying also unwraps a Svelte proxy without retaining nested references.
export function copyProfile(profile: QualityProfile): QualityProfile {
  return JSON.parse(JSON.stringify(profile)) as QualityProfile;
}
export function customizeProfile(profile: QualityProfile): QualityProfile {
  return {
    ...copyProfile(profile),
    id: `custom-${crypto.randomUUID()}`,
    name: profile.name.endsWith(' (custom)')
      ? profile.name
      : `${Array.from(profile.name).slice(0, 110).join('')} (custom)`,
    revision: 1,
    publisher: 'Local customization',
  };
}
export function profileTiming(
  profile: QualityProfile,
  assistance: TimingSettings,
): TimingSettings {
  return { ...assistance, ...profile.timing };
}
export function withTiming(
  profile: QualityProfile,
  timing: TimingSettings,
): QualityProfile {
  const thresholds = {
    cueGapMs: timing.cueGapMs,
    sceneBeforeMs: timing.sceneBeforeMs,
    sceneAfterMs: timing.sceneAfterMs,
  };
  if (
    Object.entries(thresholds).every(
      ([key, value]) =>
        profile.timing[key as keyof typeof thresholds] === value,
    )
  )
    return profile;
  return { ...customizeProfile(profile), timing: thresholds };
}
