import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  selectCues,
  reconcileSelection,
  type CueSelection,
} from './cue-selection.ts';
const order = [8, 3, 12, 7, 99].map((id) => ({ id }));
const empty: CueSelection = { ids: new Set(), primary: null, anchor: null };
test('Range selection uses a stable anchor and file order; disjoint ranges are additive', () => {
  const initial = selectCues(order, empty, 3, 'replace');
  const range = selectCues(order, initial, 7, 'range');
  assert.deepEqual([...range.ids], [3, 12, 7]);
  assert.equal(range.primary, 7);
  assert.equal(range.anchor, 3);
  assert.deepEqual([...selectCues(order, range, 12, 'range').ids], [3, 12]);
  const toggle = selectCues(order, range, 8, 'toggle');
  assert.deepEqual(
    [...selectCues(order, toggle, 99, 'addRange').ids].sort(),
    [3, 7, 8, 12, 99].sort(),
  );
  assert.deepEqual([...selectCues(order, toggle, 3, 'toggle').ids], [12, 7, 8]);
  assert.equal(selectCues(order, initial, 3, 'toggle').primary, null);
});
test('Reconciliation preserves surviving identities without selecting new unrelated rows', () => {
  const selected = selectCues(
    order,
    selectCues(order, empty, 3, 'replace'),
    99,
    'toggle',
  );
  const next = reconcileSelection(
    [{ id: 99 }, { id: 12 }, { id: 42 }],
    selected,
    12,
  );
  assert.deepEqual([...next.ids], [99, 12]);
  assert.equal(next.primary, 12);
  assert.equal(next.anchor, 99);
  assert.deepEqual([...selectCues(order, next, null, 'replace').ids], []);
});
