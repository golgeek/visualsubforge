import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cueIssueMap } from './cue-issues.ts';

test('a new report replaces both severity groups, including an empty report', () => {
  const first = cueIssueMap({ advisories: ['7'], violations: ['9', '11'] });
  const second = cueIssueMap({ advisories: ['9'], violations: [] });
  assert.deepEqual(
    [...first],
    [
      ['7', 'advisory'],
      ['9', 'violation'],
      ['11', 'violation'],
    ],
  );
  assert.deepEqual([...second], [['9', 'advisory']]);
  assert.equal(cueIssueMap({ advisories: [], violations: [] }).size, 0);
});
