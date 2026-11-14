import { clamp } from '../timeline/timeline.ts';
import { layoutPresets, type LayoutPreset } from './layout-presets.ts';

export interface PaneLayoutInput {
  workspaceHeight: number;
  topPct: number;
  editorPct: number;
  preset?: LayoutPreset;
  showQualityMetrics: boolean;
  hasCue: boolean;
  editorBarHeight: number;
  editorFooterHeight: number;
  qualityReadoutHeight: number;
  transportHeight: number;
}

// CSS pixels. Reserve controls, two text lines and slot minima before applying
// user ratios. Two six-pixel splitters and sixteen pixels of padding are fixed.
export function paneLayout(input: PaneLayoutInput) {
  const editorMinimum = Math.max(
    132,
    input.editorBarHeight +
      input.editorFooterHeight +
      64 +
      12 +
      (input.showQualityMetrics && input.hasCue
        ? input.qualityReadoutHeight + 6
        : 0),
  );
  const tableMinimum = 160;
  const transportPixels = Math.max(38, input.transportHeight);
  const minimum = 120 + tableMinimum + editorMinimum + transportPixels + 12;
  const paneHeight = Math.max(minimum, input.workspaceHeight - 16);
  const topPixels = clamp(
    (paneHeight * input.topPct) / 100,
    120,
    paneHeight - tableMinimum - transportPixels - 12 - editorMinimum,
  );
  const editorPixels = clamp(
    (paneHeight * input.editorPct) / 100,
    editorMinimum,
    paneHeight - topPixels - tableMinimum - transportPixels - 12,
  );
  const tablePixels =
    paneHeight - topPixels - editorPixels - transportPixels - 12;
  const slots = {
    preview: { minimum: 120, pixels: topPixels },
    transport: { minimum: transportPixels, pixels: transportPixels },
    cues: { minimum: tableMinimum, pixels: tablePixels },
    editor: { minimum: editorMinimum, pixels: editorPixels },
  };
  const rows = layoutPresets[input.preset ?? 'timing'].rows.map((slot) => ({
    slot,
    ...slots[slot],
  }));
  return {
    editorMinimum,
    tableMinimum,
    paneHeight,
    topPixels,
    editorPixels,
    transportPixels,
    tablePixels,
    minimum,
    rows,
  };
}
