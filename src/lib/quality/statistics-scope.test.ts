import { test } from 'node:test';
import assert from 'node:assert/strict';
import { statisticsSelection } from './statistics-scope.ts';
const cues = [{ id: 90 }, { id: 4 }, { id: 800 }];
test('Statistics scope maps stable IDs to original one-based positions, independent of selection order or duplicate IDs', () => {
  assert.deepEqual(statisticsSelection(cues, [800, 90, 90]), {
    selection: { range: null, excluded: [2] },
    error: '',
  });
  assert.deepEqual(statisticsSelection(cues, null).selection, {
    range: null,
    excluded: [],
  });
  assert.deepEqual(statisticsSelection(cues, []).selection, {
    range: null,
    excluded: [1, 2, 3],
  });
  assert.deepEqual(statisticsSelection([], []).selection, {
    range: null,
    excluded: [],
  });
});
test('A stale list ID cannot silently publish statistics for a different scope', () => {
  const input = statisticsSelection(cues, [90, 5]);
  assert.equal(input.selection, null);
  assert.match(input.error, /scope changed/);
});
