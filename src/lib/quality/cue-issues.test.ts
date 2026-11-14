import { test } from 'node:test';
import assert from 'node:assert/strict';
import { addIssuePage } from './cue-issues.ts';

test('issue pages merge highest severity for primary and related cues without mutating previous pages', () => {
  const finding = {
    cueId: '7',
    relatedCueId: '9',
    cueIndex: 0,
    rule: 'gap' as const,
    severity: 'advisory' as const,
    measured: 30,
    expected: '80 ms',
    explanation: 'Gap too small',
    language: null,
  };
  const first = addIssuePage(new Map(), [finding]);
  const second = addIssuePage(first, [
    { ...finding, cueId: '9', relatedCueId: '11', severity: 'violation' },
    { ...finding, cueId: '9' },
  ]);
  assert.deepEqual(
    [...first],
    [
      ['7', 'advisory'],
      ['9', 'advisory'],
    ],
  );
  assert.deepEqual(
    [...second],
    [
      ['7', 'advisory'],
      ['9', 'violation'],
      ['11', 'violation'],
    ],
  );
});
