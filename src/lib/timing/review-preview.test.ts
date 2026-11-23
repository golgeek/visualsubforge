import { test } from 'node:test';
import assert from 'node:assert/strict';
import { timingGhosts, timingDelta } from './review-preview.ts';
import type { TimingFixView, BatchTimingView } from '../client';
import {
  fixGapsSample,
  batchTimingView,
} from '../../../tests/frontend/ipc-contract-samples.ts';

test('Timing previews exclude unchecked or unsafe plans without changing accepted timing', () => {
  const view: TimingFixView = structuredClone(fixGapsSample);
  const before = structuredClone(view);
  view.rows[0]!.selected = false;
  const ghosts = timingGhosts(view);
  assert.equal(ghosts.length, view.rows.filter((row) => row.selected).length);
  assert(!ghosts.some((ghost) => ghost.id === Number(view.rows[0]!.cueId)));
  for (const ghost of ghosts) {
    const row = before.rows.find((row) => Number(row.cueId) === ghost.id)!;
    assert.equal(ghost.startMs, row.ghosts[0]!.after.startMs);
    assert.equal(ghost.endMs, row.ghosts[0]!.after.endMs);
  }
  assert.deepEqual(view.rows[0]!.unit, before.rows[0]!.unit);
  assert.deepEqual(
    timingGhosts({ ...view, selectionError: 'Unsafe coupled subset' }),
    [],
  );
  assert.deepEqual(timingGhosts({ ...view, phase: 'stale' }), []);
});

test('Shift and endpoint deltas retain direction and millisecond precision', () => {
  const view: BatchTimingView = structuredClone(batchTimingView);
  const ghost = timingGhosts(view)[0]!;
  assert(ghost);
  assert.equal(ghost.startMs, view.rows[0]!.newStartMs);
  assert.equal(ghost.oldStartMs, view.rows[0]!.oldStartMs);
  const base = {
    id: 1,
    oldStartMs: 1000,
    oldEndMs: 2000,
    startMs: 1001,
    endMs: 2001,
  };
  assert.equal(timingDelta(base), '+1 ms');
  assert.equal(
    timingDelta({ ...base, startMs: 1000, endMs: 1970 }),
    'end -30 ms',
  );
  assert.equal(
    timingDelta({ ...base, startMs: 1010, endMs: 1990 }),
    'start +10 ms · end -10 ms',
  );
});
