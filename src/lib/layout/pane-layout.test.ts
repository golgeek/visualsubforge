import { test } from 'node:test';
import assert from 'node:assert/strict';
import { paneLayout, type PaneLayoutInput } from './pane-layout.ts';
import {
  defaultLayout,
  layoutPresets,
  type LayoutPreset,
} from './layout-presets.ts';

const base: PaneLayoutInput = {
  workspaceHeight: 601,
  topPct: 13,
  editorPct: 16,
  showQualityMetrics: false,
  hasCue: true,
  editorBarHeight: 28,
  editorFooterHeight: 28,
  qualityReadoutHeight: 0,
  transportHeight: 38,
};
test('Timing reserves eight and fourteen full rows and two editor lines at desktop sizes', () => {
  for (const [workspaceHeight, rows] of [
    // Windows outer sizes lose 59 px to native chrome, then 60 px to app bars.
    [601, 8],
    [781, 14],
    // Hosted macOS reports a 64 px difference; local macOS reports 32 px.
    [596, 8],
    [776, 14],
    [628, 8],
    [808, 14],
  ]) {
    const result = paneLayout({ ...base, workspaceHeight: workspaceHeight! });
    assert.ok(result.tablePixels - 25 >= rows! * 32);
    assert.ok(result.editorPixels - 28 - 28 - 12 >= 64);
    assert.equal(
      result.rows.reduce((sum, row) => sum + row.pixels, 12),
      result.paneHeight,
    );
  }
});
test('each preset retains slot minima and the editor-above ordering', () => {
  for (const preset of Object.keys(layoutPresets) as LayoutPreset[]) {
    const result = paneLayout({
      ...base,
      ...layoutPresets[preset].defaults,
      preset,
    });
    assert.deepEqual(
      result.rows.map((row) => row.slot),
      layoutPresets[preset].rows,
    );
    for (const row of result.rows) assert.ok(row.pixels >= row.minimum - 1e-10);
  }
  assert.deepEqual(layoutPresets.editorAbove.rows, [
    'preview',
    'transport',
    'editor',
    'cues',
  ]);
  const defaults = defaultLayout();
  defaults.timing.topPct = 30;
  assert.equal(defaults.editorAbove.topPct, 13, 'presets own separate sizes');
  assert.equal(
    defaultLayout().timing.topPct,
    13,
    'reset defaults remain immutable',
  );
});
test('extreme ratios and wrapped controls preserve minima, allowing workspace overflow', () => {
  for (const workspaceHeight of [300, 644, 824]) {
    for (const topPct of [-10, 0, 32, 100, 200]) {
      for (const editorPct of [-10, 0, 25, 100, 200]) {
        const result = paneLayout({
          ...base,
          workspaceHeight,
          topPct,
          editorPct,
          showQualityMetrics: true,
          qualityReadoutHeight: 100,
          transportHeight: 80,
        });
        for (const row of result.rows)
          assert.ok(row.pixels >= row.minimum - 1e-10);
        assert.equal(result.editorMinimum, 238);
      }
    }
  }
  assert.equal(
    paneLayout({
      ...base,
      hasCue: false,
      showQualityMetrics: true,
      qualityReadoutHeight: 100,
    }).editorMinimum,
    132,
  );
});
