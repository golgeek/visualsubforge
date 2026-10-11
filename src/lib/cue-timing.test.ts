import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { DocumentSnapshot } from './client';
import {
  beginTimingEdit,
  previewBoundary,
  timingAction,
} from './cue-timing.ts';
const document: DocumentSnapshot = {
  generation: 5,
  revision: 8,
  loaded: true,
  path: 'example.srt',
  dirty: false,
  requiresSaveAs: false,
  canUndo: true,
  canRedo: false,
  cues: [{ id: 27, startMs: 1000, endMs: 2000, text: '日本語\n<b>café</b>' }],
};

test('Boundary previews leave accepted content untouched and produce one exact final edit', () => {
  const original = structuredClone(document);
  assert.ok(original.cues[0]);
  let edit = beginTimingEdit(document, 27, 2, false);
  for (const time of [900, 899, 799, 750])
    edit = previewBoundary(edit, 'start', time);
  assert.deepEqual(document, original);
  assert.deepEqual(edit.original, original.cues[0]);
  assert.deepEqual(timingAction(edit, document, 2, 27), {
    kind: 'edit',
    cueId: 27,
    startMs: 750,
    endMs: 2000,
    text: original.cues[0].text,
  });
  edit = previewBoundary(edit, 'start', 1000);
  assert.equal(timingAction(edit, document, 2, 27), null);
});

test('Timing gestures reject replaced documents, revisions, selections, media and unapplied drafts', () => {
  const edit = previewBoundary(
    beginTimingEdit(document, 27, 2, false),
    'end',
    2200,
  );
  for (const changed of [{ generation: 6 }, { revision: 9 }])
    assert.throws(
      () => timingAction(edit, { ...document, ...changed }, 2, 27),
      /cancelled/,
    );
  assert.throws(() => timingAction(edit, document, 3, 27), /cancelled/);
  assert.throws(() => timingAction(edit, document, 2, null), /cancelled/);
  assert.throws(
    () => beginTimingEdit(document, 27, 2, true),
    /Apply or revert/,
  );
  assert.throws(() => beginTimingEdit(document, 99, 2, false), /Select a cue/);
  assert.throws(() =>
    beginTimingEdit({ ...document, loaded: false }, 27, 2, false),
  );
});

test('Boundary constraints keep positive integer intervals without moving the opposite endpoint', () => {
  const edit = beginTimingEdit(document, 27, 2, false);
  assert.equal(previewBoundary(edit, 'start', -10).preview.startMs, 0);
  assert.equal(previewBoundary(edit, 'start', 3000).preview.startMs, 1999);
  assert.equal(previewBoundary(edit, 'end', 0).preview.endMs, 1001);
  assert.equal(
    previewBoundary(edit, 'end', 40_000_000_000).preview.endMs,
    35_999_999_999,
  );
  for (const time of [NaN, Infinity, 1.1])
    assert.throws(() => previewBoundary(edit, 'end', time));
});
