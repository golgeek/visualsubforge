import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  defaultColumns,
  moveColumn,
  showColumn,
  resizeColumn,
  visibleColumns,
  rowHeight,
  filterCueRows,
} from './cue-columns.ts';

test('column order, visibility and integer widths round trip without mutating defaults', () => {
  const original = defaultColumns();
  const reordered = moveColumn(original, 'cps', 'start');
  const configured = resizeColumn(
    showColumn(reordered, 'cps', true),
    'cps',
    123.6,
  );
  assert.equal(configured.items[2]?.id, 'cps');
  assert.equal(configured.items[2]?.width, 124);
  assert.equal(configured.items[2]?.visible, true);
  assert.deepEqual(JSON.parse(JSON.stringify(configured)), configured);
  assert.deepEqual(original, defaultColumns());
  assert.equal(resizeColumn(configured, 'cps', Infinity), configured);
  assert.equal(resizeColumn(configured, 'cps', -8).items[2]?.width, 40);
});
test('Issues stays first and Issues/Text cannot be hidden; ASS choices survive other formats', () => {
  let value = defaultColumns();
  value = showColumn(showColumn(value, 'issue', false), 'text', false);
  assert.deepEqual(moveColumn(value, 'issue', 'text'), value);
  assert.deepEqual(moveColumn(value, 'text', 'issue'), value);
  value = showColumn(showColumn(value, 'style', true), 'actor', true);
  assert.ok(
    !visibleColumns(value, false).some(
      (c) => c.id === 'style' || c.id === 'actor',
    ),
  );
  assert.ok(visibleColumns(value, true).some((c) => c.id === 'actor'));
  assert.ok(visibleColumns(value, false).some((c) => c.id === 'text'));
  assert.equal(rowHeight('compact'), 24);
  assert.equal(rowHeight('comfortable'), 32);
});
test('text, file-number and issue filters retain source indices and IDs', () => {
  const cues = [
    { id: 80, text: 'First', startMs: 0, endMs: 1 },
    { id: 3, text: 'SECOND', startMs: 1, endMs: 2 },
  ];
  const issues = new Map([['3', 'violation']]);
  assert.deepEqual(filterCueRows(cues, '#2', false, issues), [
    { cue: cues[1], index: 1 },
  ]);
  assert.equal(filterCueRows(cues, '#3', false, issues).length, 0);
  assert.equal(filterCueRows(cues, 'second', true, issues)[0]?.cue.id, 3);
  assert.equal(filterCueRows(cues, 'first', true, issues).length, 0);
});
