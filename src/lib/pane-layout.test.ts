import { test } from 'node:test';
import assert from 'node:assert/strict';
import { paneLayout, type PaneLayoutInput } from './pane-layout.ts';

const base: PaneLayoutInput = {
  workspaceHeight: 600,
  topPct: 32,
  editorPct: 25,
  showQualityMetrics: false,
  hasCue: true,
  editorBarHeight: 0,
  editorFooterHeight: 0,
  qualityReadoutHeight: 0,
  transportHeight: 0,
  cueOperationsHeight: 0,
  cueFollowingHeight: 0,
};
test('pane layout retains ordinary ratios and reserves the two splitters', () => {
  assert.deepEqual(paneLayout(base), {
    editorMinimum: 130,
    tableMinimum: 220,
    paneHeight: 584,
    topPixels: 186.88,
    editorPixels: 146,
  });
});
test('wrapped controls and metrics retain content room even in a small workspace', () => {
  const layout = paneLayout({
    ...base,
    workspaceHeight: 300,
    showQualityMetrics: true,
    editorBarHeight: 100,
    editorFooterHeight: 60,
    qualityReadoutHeight: 100,
    transportHeight: 80,
    cueOperationsHeight: 80,
    cueFollowingHeight: 100,
  });
  assert.equal(layout.editorMinimum, 330);
  assert.equal(layout.tableMinimum, 349);
  assert.equal(layout.paneHeight, 811);
  assert.equal(layout.topPixels, 120);
  assert.equal(layout.editorPixels, 330);
});
test('extreme splitter positions preserve preview, table and editor minima', () => {
  for (const topPct of [-10, 0, 32, 100, 200]) {
    for (const editorPct of [-10, 0, 25, 100, 200]) {
      const layout = paneLayout({ ...base, topPct, editorPct });
      assert.ok(layout.topPixels >= 120);
      assert.ok(layout.editorPixels >= layout.editorMinimum);
      assert.ok(
        layout.paneHeight - layout.topPixels - layout.editorPixels - 12 >=
          layout.tableMinimum - 1e-10,
      );
    }
  }
  const noCue = paneLayout({
    ...base,
    showQualityMetrics: true,
    hasCue: false,
    qualityReadoutHeight: 500,
  });
  assert.equal(
    noCue.editorMinimum,
    190,
    'Hidden cue metrics reserve no readout height',
  );
});
