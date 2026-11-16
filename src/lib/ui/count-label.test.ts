import { test } from 'node:test';
import assert from 'node:assert/strict';
import { countLabel } from './count-label.ts';
test('count labels handle singular, zero, unknown and irregular nouns', () => {
  assert.equal(countLabel(1, 'issue'), '1 issue');
  assert.equal(countLabel(0, 'line'), '0 lines');
  assert.equal(countLabel(2, 'line'), '2 lines');
  assert.equal(countLabel('—', 'issue'), '— issues');
  assert.equal(countLabel(1, 'match', 'matches'), '1 match');
});
