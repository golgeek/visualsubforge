import { formatTime, parseTime } from '../timeline/timeline.ts';

/** Nominal cadence only: VFR boundaries need video review. Quantize once at
 * the document format boundary (1 ms for SRT/VTT, 10 ms for ASS/SSA). */
export function nudgeTimecode(
  value: string,
  direction: -1 | 1,
  frameRate: number,
  timeStepMs: number,
): string {
  if (!Number.isFinite(frameRate) || frameRate < 1 || frameRate > 1000)
    throw new Error(
      'The video frame rate is unavailable. Enter an exact time.',
    );
  if (!Number.isInteger(timeStepMs) || timeStepMs < 1)
    throw new Error('The subtitle time precision is unavailable.');
  const next = Math.max(0, parseTime(value) + (direction * 1000) / frameRate);
  return formatTime(Math.round(next / timeStepMs) * timeStepMs);
}
