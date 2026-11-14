import { test } from 'node:test';
import assert from 'node:assert/strict';
import { timingGhosts, timingDelta } from './review-preview.ts';
import type { GapView, BatchTimingView } from '../client';
import {
  gapView,
  batchTimingView,
} from '../../../tests/frontend/ipc-contract-samples.ts';

test('Timing previews exclude unchecked or unsafe plans without changing accepted timing', () => {
  const view: GapView = structuredClone(gapView);
  const before = structuredClone(view);
  view.rows[0]!.selected = false;
  const ghosts = timingGhosts(view);
  assert.equal(ghosts.length, view.rows.filter((row) => row.selected).length);
  assert(!ghosts.some((ghost) => ghost.id === Number(view.rows[0]!.cueId)));
  for (const ghost of ghosts) {
    const row = before.rows.find((row) => Number(row.cueId) === ghost.id)!;
    assert.equal(ghost.startMs, row.before.data.startMs);
    assert.equal(ghost.endMs, row.newEndMs);
  }
  assert.deepEqual(view.rows[0]!.before, before.rows[0]!.before);
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
