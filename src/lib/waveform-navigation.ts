import { clamp, viewport } from './timeline.ts';

interface WheelInput {
  deltaX: number;
  deltaY: number;
  deltaMode: number;
  shiftKey: boolean;
  offsetX: number;
}

// Pure viewport navigation. No transport position or document state is changed.
// Dominant-axis selection avoids accidental zoom during diagonal trackpad pans.
export function wheelViewport(
  start: number,
  span: number,
  duration: number,
  width: number,
  input: WheelInput,
): { start: number; span: number } | null {
  if (
    duration <= 0 ||
    width <= 0 ||
    span <= 0 ||
    ![
      start,
      span,
      duration,
      width,
      input.deltaX,
      input.deltaY,
      input.offsetX,
    ].every(Number.isFinite)
  )
    return null;
  const unit = input.deltaMode === 1 ? 16 : input.deltaMode === 2 ? width : 1;
  const dx = input.deltaX * unit;
  const dy = input.deltaY * unit;
  if (dx === 0 && dy === 0) return null;
  // Shift+wheel also provides horizontal navigation on a single-axis mouse.
  if (input.shiftKey || Math.abs(dx) > Math.abs(dy)) {
    const delta = dx !== 0 ? dx : dy;
    return viewport(start + (delta / width) * span, span, duration);
  }
  const anchor = clamp(input.offsetX / width, 0, 1);
  const nextSpan = viewport(
    0,
    span * Math.exp(clamp(dy * 0.002, -2, 2)),
    duration,
  ).span;
  return viewport(start + anchor * (span - nextSpan), nextSpan, duration);
}

// Keep upcoming audio visible instead of paging at the right edge.
export function playbackViewportStart(
  position: number,
  span: number,
  duration: number,
): number {
  return viewport(position - span * 0.4, span, duration).start;
}

// A stable, padded request window avoids discarding every asynchronous peak
// response as the visible viewport follows playback. Preserve pixel density.
export function waveformRequestWindow(
  start: number,
  span: number,
  duration: number,
  width: number,
) {
  const step = span / 2;
  const left = Math.max(0, Math.floor(start / step) * step - step);
  const end = Math.min(duration, left + span * 2.5);
  return {
    start: left,
    end,
    pixels: Math.min(
      4096,
      Math.max(1, Math.ceil((width * (end - left)) / span)),
    ),
  };
}
