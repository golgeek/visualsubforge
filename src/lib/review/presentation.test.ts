import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  diffParts,
  reviewRows,
  retainedChecks,
  exportProgress,
  type ReviewRow,
} from './presentation.ts';
test('each side retains its own changed breaks and handles one-sided cues', () => {
  const words = [
    { kind: 'removed' as const, text: 'one\ntwo' },
    { kind: 'added' as const, text: 'three\nfour' },
  ];
  assert.equal(
    diffParts(words, 'original', true)
      .map((p) => p.text)
      .join(''),
    'one↵two',
  );
  assert.equal(
    diffParts(words, 'revised')
      .map((p) => p.text)
      .join(''),
    'three↵\nfour',
  );
  assert.deepEqual(diffParts(words.slice(1), 'original'), []);
  assert.deepEqual(diffParts(words.slice(0, 1), 'revised'), []);
});
test('filter membership includes split members and the successor of a removal', () => {
  const row: ReviewRow = {
    index: 0,
    key: 'a',
    occurrence: 0,
    sourceIndex: 0,
    revised: [10, 11],
    nextCue: null,
    kind: 'split',
    hasThreads: true,
    open: false,
    block: null,
    shiftMs: null,
  };
  const rows = [
    row,
    {
      ...row,
      key: 'b',
      revised: [],
      kind: 'removed' as const,
      nextCue: 12,
      hasThreads: false,
    },
  ];
  const result = reviewRows(rows);
  assert.deepEqual([...result.changed], [10, 11, 12]);
  assert.deepEqual([...result.threaded], [10, 11]);
  assert.equal(result.removals.get(12)?.[0]?.key, 'b');
  const marks = retainedChecks(rows, new Set(['a/0', 'a/1', 'old/0']));
  assert.deepEqual([...marks], ['a/0']);
  assert.deepEqual(exportProgress(rows, marks), {
    total: 2,
    checked: 1,
    unchecked: [rows[1]],
  });
});
