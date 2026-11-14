import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  bufferVisibility,
  distanceHatch,
  gapMarks,
  cutDistances,
  cutDistanceTags,
  cueDensity,
} from './timeline-marks.ts';
import { defaultTiming } from '../timing/magnetic-timing.ts';
const cue = (id: number, startMs: number, endMs: number) => ({
  id,
  startMs,
  endMs,
  text: '',
});
test('rest brackets remain visible; only a drag expands bands, including when snapping is bypassed', () => {
  assert.deepEqual(bufferVisibility(false), {
    brackets: true,
    shotBands: false,
    cueBands: false,
  });
  assert.deepEqual(bufferVisibility(true), {
    brackets: true,
    shotBands: true,
    cueBands: true,
  });
  assert.equal(distanceHatch(59, 0.1), false);
  assert.equal(distanceHatch(-60, 0.1), true);
});
test('gap lane sorts time without mutating file order and classifies exact minimum and overlaps', () => {
  const cues = [cue(8, 250, 400), cue(9, 0, 100), cue(10, 180, 300)];
  assert.deepEqual(
    gapMarks(cues, 80).map((g) => [g.milliseconds, g.severity]),
    [
      [80, 'neutral'],
      [-50, 'violation'],
    ],
  );
  assert.equal(cues[0]!.id, 8);
  assert.equal(
    gapMarks([cue(1, 0, 100), cue(2, 100, 200)], 80)[0]!.severity,
    'warning',
  );
});
test('cut warnings honor open asymmetric boundaries, nearest cuts, duplicates and zero buffers', () => {
  const cues = [cue(1, 870, 1130), cue(2, 950, 1000)];
  assert.equal(cutDistances(cues, [1, 1], defaultTiming).length, 2);
  assert.equal(
    cutDistances(cues, [1], {
      ...defaultTiming,
      sceneBeforeMs: 0,
      sceneAfterMs: 0,
    }).length,
    0,
  );
  assert.deepEqual(
    cutDistances([cue(3, 1050, 2000)], [1.1, 1], defaultTiming).map(
      (x) => x.cutMs,
    ),
    [1000],
  );
});
test('density is bounded and counts overlapping spans without expanding per-cue pixels', () => {
  assert.deepEqual(
    cueDensity([cue(1, 0, 500), cue(2, 250, 750)], 1, 4),
    [1, 2, 1, 0],
  );
  assert.equal(cueDensity([], 10, 100000).length, 1024);
  assert.deepEqual(cueDensity([], 0, 2), [0, 0]);
});

test('coincident cut tags retain the nearest distance and count all affected edges', () => {
  const warnings = cutDistances(
    [cue(1, 940, 990), cue(2, 2000, 2075)],
    [1, 2.1],
    defaultTiming,
  );
  assert.deepEqual(cutDistanceTags(warnings), [
    { cutMs: 1000, nearestMs: 10, edges: 2 },
    { cutMs: 2100, nearestMs: 25, edges: 2 },
  ]);
});
