import { clamp } from './timeline.ts';

export interface PaneLayoutInput {
  workspaceHeight: number;
  topPct: number;
  editorPct: number;
  showQualityMetrics: boolean;
  hasCue: boolean;
  editorBarHeight: number;
  editorFooterHeight: number;
  qualityReadoutHeight: number;
  transportHeight: number;
  cueOperationsHeight: number;
  cueFollowingHeight: number;
}

// Reserve measured controls and usable content before applying split ratios.
// Dimensions are logical CSS pixels; the two splitters occupy 12 pixels.
export function paneLayout(input: PaneLayoutInput) {
  const editorMinimum = Math.max(
    input.showQualityMetrics ? 190 : 130,
    input.editorBarHeight +
      input.editorFooterHeight +
      64 +
      (input.showQualityMetrics && input.hasCue
        ? input.qualityReadoutHeight + 6
        : 0),
  );
  const tableMinimum = Math.max(
    220,
    input.transportHeight +
      input.cueOperationsHeight +
      input.cueFollowingHeight +
      25 +
      64,
  );
  const paneHeight = Math.max(
    132 + tableMinimum + editorMinimum,
    input.workspaceHeight - 16,
  );
  const topPixels = clamp(
    (paneHeight * input.topPct) / 100,
    120,
    paneHeight - tableMinimum - 12 - editorMinimum,
  );
  const editorPixels = clamp(
    (paneHeight * input.editorPct) / 100,
    editorMinimum,
    paneHeight - topPixels - tableMinimum - 12,
  );
  return { editorMinimum, tableMinimum, paneHeight, topPixels, editorPixels };
}
