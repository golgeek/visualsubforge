import type { Cue, TimingSettings } from './client';
export type { TimingSettings } from './client';
import type { CueBoundary } from './cue-timing';

// Optional editing aid, not an enforced delivery standard.
export const defaultTiming: TimingSettings = {
  enabled: false,
  snapCues: true,
  snapScenes: true,
  cueGapMs: 80,
  sceneBeforeMs: 130,
  sceneAfterMs: 130,
  snapDistancePx: 8,
};
export interface SnapResult {
  milliseconds: number;
  target: 'cue gap' | 'scene buffer' | null;
}

// The pointer proposes integer milliseconds. Snap only to valid endpoints;
// ordinary edits may still overlap and keyboard nudges remain exact.
export function snapBoundary(
  milliseconds: number,
  boundary: CueBoundary,
  oppositeMs: number,
  selected: number | null,
  cues: readonly Cue[],
  cuts: readonly number[],
  settings: TimingSettings,
  toleranceMs: number,
  durationMs: number,
  bypass = false,
): SnapResult {
  const unchanged: SnapResult = { milliseconds, target: null };
  if (
    !settings.enabled ||
    bypass ||
    !Number.isFinite(toleranceMs) ||
    toleranceMs < 0
  )
    return unchanged;
  const others = cues.filter((c) => c.id !== selected);
  let best: SnapResult = unchanged;
  let distance = Infinity;
  const consider = (
    candidate: number,
    target: NonNullable<SnapResult['target']>,
  ) => {
    candidate = Math.round(candidate);
    if (
      !Number.isSafeInteger(candidate) ||
      candidate < 0 ||
      candidate > durationMs ||
      (boundary === 'start' ? candidate >= oppositeMs : candidate <= oppositeMs)
    )
      return;
    const delta = Math.abs(candidate - milliseconds);
    if (delta > toleranceMs || delta > distance) return;
    if (
      settings.snapScenes &&
      cuts.some((seconds) => {
        const cut = Math.round(seconds * 1000);
        return (
          candidate > cut - settings.sceneBeforeMs &&
          candidate < cut + settings.sceneAfterMs
        );
      })
    )
      return;
    // A magnetic landing must not be inside another cue or its requested gap.
    // This does not prohibit an intentional overlap made with bypass/manual edits.
    if (
      settings.snapCues &&
      others.some((c) =>
        boundary === 'start'
          ? candidate >= c.startMs && candidate < c.endMs + settings.cueGapMs
          : candidate > c.startMs - settings.cueGapMs && candidate <= c.endMs,
      )
    )
      return;
    // Stable ties: cue gaps before scene buffers, then the earlier time.
    if (
      delta < distance ||
      (delta === distance &&
        (target === best.target
          ? candidate < best.milliseconds
          : target === 'cue gap'))
    ) {
      distance = delta;
      best = { milliseconds: candidate, target };
    }
  };
  if (settings.snapCues)
    for (const cue of others)
      consider(
        boundary === 'start'
          ? cue.endMs + settings.cueGapMs
          : cue.startMs - settings.cueGapMs,
        'cue gap',
      );
  if (settings.snapScenes)
    for (const seconds of cuts)
      consider(
        seconds * 1000 +
          (boundary === 'start'
            ? settings.sceneAfterMs
            : -settings.sceneBeforeMs),
        'scene buffer',
      );
  return best;
}
