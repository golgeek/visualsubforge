import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  cueIssueMap,
  reconcileCueIssues,
  type IssueSeverity,
} from './cue-issues.ts';

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

test('reconcile a missed revision of a 10k index with writes only for changed IDs', () => {
  const advisories = Array.from({ length: 10000 }, (_, i) => String(i));
  const writes: string[] = [];
  class ObservedMap extends Map<string, IssueSeverity> {
    override set(id: string, severity: IssueSeverity) {
      writes.push(id);
      return super.set(id, severity);
    }
    override delete(id: string) {
      writes.push(id);
      return super.delete(id);
    }
  }
  const displayed = new ObservedMap(
    cueIssueMap({ advisories, violations: [] }),
  );
  writes.length = 0;
  const latest = { advisories: advisories.slice(2), violations: ['1', 'new'] };
  reconcileCueIssues(displayed, latest);
  assert.deepEqual(new Map(displayed), cueIssueMap(latest));
  assert.deepEqual(writes, ['0', '1', 'new']);
  writes.length = 0;
  reconcileCueIssues(displayed, latest);
  assert.deepEqual(writes, []);
});
