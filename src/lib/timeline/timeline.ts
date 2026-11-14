export function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}
export function timeToX(
  time: number,
  start: number,
  span: number,
  width: number,
): number {
  return ((time - start) / span) * width;
}
export function xToTime(
  x: number,
  start: number,
  span: number,
  width: number,
): number {
  return start + (clamp(x, 0, width) / Math.max(1, width)) * span;
}
export function viewport(
  start: number,
  span: number,
  duration: number,
): { start: number; span: number } {
  const boundedSpan = clamp(span, 0.1, Math.max(0.1, duration));
  return {
    start: clamp(start, 0, Math.max(0, duration - boundedSpan)),
    span: boundedSpan,
  };
}
export function orderedRange(
  a: number,
  b: number,
  duration: number,
): [number, number] {
  return [
    clamp(Math.min(a, b), 0, duration),
    clamp(Math.max(a, b), 0, duration),
  ];
}
export function formatTime(ms: number): string {
  const n = Math.round(ms);
  return `${String(Math.floor(n / 3600000)).padStart(2, '0')}:${String(Math.floor(n / 60000) % 60).padStart(2, '0')}:${String(Math.floor(n / 1000) % 60).padStart(2, '0')},${String(n % 1000).padStart(3, '0')}`;
}
export function parseTime(text: string): number {
  const match = /^(\d{2,4}):(\d{2}):(\d{2})[,.](\d{3})$/.exec(text);
  if (!match) throw new Error('Use HH:MM:SS,mmm for cue times.');
  const h = Number(match[1]),
    m = Number(match[2]),
    s = Number(match[3]),
    ms = Number(match[4]);
  if (m >= 60 || s >= 60)
    throw new Error('Minutes and seconds must be below 60.');
  return h * 3600000 + m * 60000 + s * 1000 + ms;
}
