import type { Cue } from '../client';
import { timeToX, xToTime } from './timeline.ts';

type Point = { x: number; y: number };
type View = { start: number; span: number; width: number };

export function cueAt(cues: Cue[], selected: number | null, seconds: number) {
  const ms = seconds * 1000;
  let last: Cue | undefined;
  for (const cue of cues) {
    if (ms < cue.startMs || ms >= cue.endMs) continue;
    if (cue.id === selected) return cue;
    last = cue;
  }
  return last;
}

export function exactRange(
  cue: Cue | undefined,
  selected: number | null,
  range: [number, number] | null,
) {
  return (
    !!cue &&
    cue.id === selected &&
    range?.[0] === cue.startMs / 1000 &&
    range[1] === cue.endMs / 1000
  );
}

export function edgeAt(point: Point, view: View, cues: Cue[]) {
  if (point.y < 20) return null;
  let closest: {
    cue: Cue;
    boundary: 'start' | 'end';
    distance: number;
  } | null = null;
  for (const cue of cues)
    for (const boundary of ['start', 'end'] as const) {
      const seconds = cue[boundary === 'start' ? 'startMs' : 'endMs'] / 1000;
      const distance = Math.abs(
        timeToX(seconds, view.start, view.span, view.width) - point.x,
      );
      if (distance <= 6 && (!closest || distance < closest.distance))
        closest = { cue, boundary, distance };
    }
  return closest;
}

export function waveformCursor(
  point: Point | null,
  view: View,
  cues: Cue[],
  selected: number | null,
  range: [number, number] | null,
  {
    enabled,
    drag,
    canResize,
    canSelect,
  }: {
    enabled: boolean;
    drag: 'edge' | 'range' | null;
    canResize: boolean;
    canSelect: boolean;
  },
) {
  if (!enabled) return 'default';
  if (drag === 'edge') return 'ew-resize';
  if (drag || !point || point.y < 20) return 'crosshair';
  if (canResize && edgeAt(point, view, cues)) return 'ew-resize';
  const cue =
    canSelect &&
    cueAt(cues, selected, xToTime(point.x, view.start, view.span, view.width));
  return cue && !exactRange(cue, selected, range) ? 'pointer' : 'crosshair';
}
