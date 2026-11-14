import type { CanvasPalette } from '../ui/canvas-palette';
import type { Cue, TimingSettings } from '../client';
import type { SnapGuide } from '../timing/magnetic-timing';
import {
  cutDistances,
  cutDistanceTags,
  distanceHatch,
  type GapMark,
} from './timeline-marks';

export function hatch(
  context: CanvasRenderingContext2D,
  left: number,
  right: number,
  top: number,
  bottom: number,
  color: string,
) {
  if (right <= left || bottom <= top) return;
  context.save();
  context.beginPath();
  context.rect(left, top, right - left, bottom - top);
  context.clip();
  context.strokeStyle = color;
  context.lineWidth = 1;
  for (let x = left - (bottom - top); x < right; x += 6) {
    context.beginPath();
    context.moveTo(x, bottom);
    context.lineTo(x + bottom - top, top);
    context.stroke();
  }
  context.restore();
}
export function drawTimelineMarks(
  context: CanvasRenderingContext2D,
  colors: CanvasPalette,
  options: {
    width: number;
    height: number;
    start: number;
    span: number;
    cuts: readonly number[];
    cues: readonly Cue[];
    gaps: readonly GapMark[];
    timing: TimingSettings;
    guides: readonly SnapGuide[];
  },
) {
  const { width, height, start, span, cuts, cues, gaps, timing, guides } =
    options;
  const x = (ms: number) => ((ms / 1000 - start) / span) * width;
  const bottom = Math.max(20, height - 12);
  context.save();
  context.setLineDash([]);
  context.lineWidth = 1;
  context.strokeStyle = colors.cut;
  context.fillStyle = colors.cut;
  for (const cut of cuts) {
    const px = x(cut * 1000);
    if (px < 0 || px > width) continue;
    context.beginPath();
    context.moveTo(px, 15);
    context.lineTo(px, bottom);
    context.stroke();
    context.beginPath();
    context.moveTo(px - 3, 14);
    context.lineTo(px + 3, 14);
    context.lineTo(px, 18);
    context.closePath();
    context.fill();
    const left = x(cut * 1000 - timing.sceneBeforeMs),
      right = x(cut * 1000 + timing.sceneAfterMs);
    context.beginPath();
    context.moveTo(left, 17);
    context.lineTo(left, 19);
    context.lineTo(right, 19);
    context.lineTo(right, 17);
    context.stroke();
  }
  context.font = `9px ${colors.fontMono}`;
  const warnings = cutDistances(cues, cuts, timing);
  for (const warning of warnings) {
    const edge = x(warning.edgeMs),
      cut = x(warning.cutMs);
    if (Math.max(edge, cut) < 0 || Math.min(edge, cut) > width) continue;
    if (distanceHatch(warning.distanceMs, width / span / 1000))
      hatch(
        context,
        Math.max(0, Math.min(edge, cut)),
        Math.min(width, Math.max(edge, cut)),
        21,
        bottom,
        colors.warning,
      );
  }
  for (const tag of cutDistanceTags(warnings)) {
    const cut = x(tag.cutMs);
    if (cut < 0 || cut > width) continue;
    const label = `${tag.nearestMs} ms${tag.edges > 1 ? ` · ${tag.edges} edges` : ''}`;
    const labelWidth = context.measureText(label).width + 6;
    const left = Math.max(0, Math.min(width - labelWidth, cut + 4));
    context.fillStyle = colors.background;
    context.fillRect(left, 22, labelWidth, 12);
    context.fillStyle = colors.warning;
    context.fillText(label, left + 3, 31);
  }
  context.fillStyle = colors.background;
  context.fillRect(0, bottom, width, height - bottom);
  for (const gap of gaps) {
    const left = Math.max(0, x(gap.fromMs)),
      right = Math.min(width, x(gap.toMs));
    if (right < left || right < 0 || left > width) continue;
    const color =
      gap.severity === 'violation'
        ? colors.error
        : gap.severity === 'warning'
          ? colors.warning
          : colors.rulerLine;
    context.fillStyle = color;
    context.fillRect(left, bottom, Math.max(1, right - left), 3);
    const label = `${gap.milliseconds} ms`;
    if (right - left > context.measureText(label).width + 6) {
      context.fillStyle = gap.severity === 'neutral' ? colors.rulerText : color;
      context.fillText(label, left + 3, height - 1);
    }
  }
  context.strokeStyle = colors.accent;
  context.lineWidth = 1;
  context.setLineDash([4, 3]);
  for (const guide of guides) {
    const px = x(guide.milliseconds);
    if (px < 0 || px > width) continue;
    context.beginPath();
    context.moveTo(px, 20);
    context.lineTo(px, bottom);
    context.stroke();
    const labelWidth = context.measureText(guide.label).width + 8;
    const left = Math.max(0, Math.min(width - labelWidth, px + 5));
    context.fillStyle = colors.background;
    const top = Math.min(36, Math.max(21, bottom - 14));
    context.fillRect(left, top, labelWidth, 13);
    context.fillStyle = colors.accent;
    context.fillText(guide.label, left + 4, top + 10);
  }
  context.restore();
}
