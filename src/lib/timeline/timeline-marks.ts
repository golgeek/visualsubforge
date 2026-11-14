import type { Cue, TimingSettings } from '../client';

export interface GapMark {
  fromMs: number;
  toMs: number;
  milliseconds: number;
  severity: 'neutral' | 'warning' | 'violation';
}
/** Chronological neighbours, independent of document order. Display only; Rust owns findings. */
export function gapMarks(cues: readonly Cue[], minimumMs: number): GapMark[] {
  const ordered = [...cues].sort(
    (a, b) => a.startMs - b.startMs || a.id - b.id,
  );
  return ordered.slice(1).map((cue, index) => {
    const previous = ordered[index]!;
    const milliseconds = cue.startMs - previous.endMs;
    return {
      fromMs: Math.min(previous.endMs, cue.startMs),
      toMs: Math.max(previous.endMs, cue.startMs),
      milliseconds,
      severity:
        milliseconds < 0
          ? 'violation'
          : milliseconds < minimumMs
            ? 'warning'
            : 'neutral',
    };
  });
}
export function bufferVisibility(dragging: boolean) {
  return { brackets: true, shotBands: dragging, cueBands: dragging };
}
export function distanceHatch(distanceMs: number, pixelsPerMs: number) {
  return Math.abs(distanceMs * pixelsPerMs) >= 6;
}
export interface CutDistance {
  cueId: number;
  edgeMs: number;
  cutMs: number;
  distanceMs: number;
}
/** Nearest offending cut per endpoint, with logarithmic lookup in sorted cuts. */
export function cutDistances(
  cues: readonly Cue[],
  cuts: readonly number[],
  timing: TimingSettings,
): CutDistance[] {
  const sorted = [...new Set(cuts.map((cut) => Math.round(cut * 1000)))].sort(
    (a, b) => a - b,
  );
  const result: CutDistance[] = [];
  for (const cue of cues)
    for (const edgeMs of [cue.startMs, cue.endMs]) {
      let lo = 0,
        hi = sorted.length;
      while (lo < hi) {
        const middle = (lo + hi) >>> 1;
        if (sorted[middle]! < edgeMs) lo = middle + 1;
        else hi = middle;
      }
      const candidates = [sorted[lo - 1], sorted[lo]].filter(
        (cut): cut is number =>
          cut !== undefined &&
          edgeMs > cut - timing.sceneBeforeMs &&
          edgeMs < cut + timing.sceneAfterMs,
      );
      candidates.sort(
        (a, b) => Math.abs(edgeMs - a) - Math.abs(edgeMs - b) || a - b,
      );
      if (candidates[0] !== undefined)
        result.push({
          cueId: cue.id,
          edgeMs,
          cutMs: candidates[0],
          distanceMs: edgeMs - candidates[0],
        });
    }
  return result;
}
/** Bounded density bins using a difference array, O(cues + bins), not cue × bins. */
export function cueDensity(
  cues: readonly Cue[],
  duration: number,
  bins = 256,
): number[] {
  const count = Math.max(1, Math.min(1024, Math.floor(bins)));
  const differences = new Array<number>(count + 1).fill(0);
  if (!(duration > 0)) return differences.slice(0, count);
  for (const cue of cues) {
    if (cue.endMs < 0 || cue.startMs > duration * 1000) continue;
    const left = Math.max(
      0,
      Math.min(count - 1, Math.floor((cue.startMs / 1000 / duration) * count)),
    );
    const right = Math.max(
      left + 1,
      Math.min(count, Math.ceil((cue.endMs / 1000 / duration) * count)),
    );
    differences[left]!++;
    differences[right]!--;
  }
  let active = 0;
  return differences.slice(0, count).map((delta) => (active += delta));
}

/** One tag per cut: nearest distance plus the count, so endpoint tags cannot overwrite each other. */
export function cutDistanceTags(warnings: readonly CutDistance[]) {
  const tags = new Map<
    number,
    { cutMs: number; nearestMs: number; edges: number }
  >();
  for (const warning of warnings) {
    const tag = tags.get(warning.cutMs);
    if (tag) {
      tag.nearestMs = Math.min(tag.nearestMs, Math.abs(warning.distanceMs));
      tag.edges++;
    } else
      tags.set(warning.cutMs, {
        cutMs: warning.cutMs,
        nearestMs: Math.abs(warning.distanceMs),
        edges: 1,
      });
  }
  return [...tags.values()];
}
