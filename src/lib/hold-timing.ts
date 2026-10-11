import type { DocumentSnapshot, MediaStatus } from './client';

type DocumentVersion = Pick<DocumentSnapshot, 'generation' | 'revision'>;
type Clock = Pick<
  MediaStatus,
  'session' | 'loaded' | 'paused' | 'seeking' | 'positionSeconds'
>;
export interface HeldTiming extends DocumentVersion {
  session: number;
  start: number;
  last: number;
}
export function beginHeldTiming(
  document: DocumentVersion,
  clock: Clock,
): HeldTiming | null {
  const position = clock.positionSeconds;
  if (
    !clock.loaded ||
    clock.paused ||
    clock.seeking ||
    position === null ||
    !Number.isFinite(position) ||
    position < 0
  )
    return null;
  return {
    generation: document.generation,
    revision: document.revision,
    session: clock.session,
    start: position,
    last: position,
  };
}
export function advanceHeldTiming(
  held: HeldTiming,
  document: DocumentVersion,
  clock: Clock,
): HeldTiming | null {
  const current = beginHeldTiming(document, clock);
  if (
    !current ||
    current.generation !== held.generation ||
    current.revision !== held.revision ||
    current.session !== held.session ||
    current.start < held.last
  )
    return null;
  return { ...held, last: current.start };
}
export function finishHeldTiming(
  held: HeldTiming,
  document: DocumentVersion,
  clock: Clock,
): [number, number] | null {
  const current = advanceHeldTiming(held, document, clock);
  if (
    !current ||
    Math.round(current.last * 1000) <= Math.round(current.start * 1000)
  )
    return null;
  return [current.start, current.last];
}

// Gesture duration is monotonic wall time, independent of playback rate or the
// 100 ms media poll. Cue endpoints still come from the validated source clock.
export const timingHoldThresholdMs = 400;
export function resolveTimingRelease(
  held: HeldTiming,
  document: DocumentVersion,
  clock: Clock,
  heldForMs: number,
): { kind: 'pause' } | { kind: 'create'; range: [number, number] } | null {
  if (
    !Number.isFinite(heldForMs) ||
    heldForMs < 0 ||
    !advanceHeldTiming(held, document, clock)
  )
    return null;
  if (heldForMs < timingHoldThresholdMs) return { kind: 'pause' };
  const range = finishHeldTiming(held, document, clock);
  return range ? { kind: 'create', range } : null;
}
