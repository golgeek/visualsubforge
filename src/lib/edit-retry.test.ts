import { test } from 'node:test';
import assert from 'node:assert/strict';
import { retryEdit, type CueEdit } from './edit-retry.ts';
import type { DocumentSnapshot } from './client';
const base: DocumentSnapshot = {
  loaded: true,
  generation: 1,
  revision: 1,
  path: null,
  dirty: false,
  requiresSaveAs: false,
  canUndo: false,
  canRedo: false,
  cues: [{ id: 7, startMs: 1000, endMs: 2000, text: 'base', style: 'Default' }],
};
const edit: CueEdit = {
  kind: 'editStyled',
  cueId: 7,
  startMs: 1000,
  endMs: 2000,
  text: 'draft',
  style: 'Default',
};
test('retry merges independent text, endpoint and style changes', () => {
  const current = {
    ...base,
    revision: 2,
    cues: [{ ...base.cues[0]!, startMs: 1200, style: 'Other' }],
  };
  assert.deepEqual(retryEdit(base, current, edit), {
    ...edit,
    startMs: 1200,
    style: 'Other',
  });
  const timing = { ...edit, text: 'base', endMs: 2500 };
  assert.deepEqual(
    retryEdit(
      base,
      { ...current, cues: [{ ...current.cues[0]!, text: 'other text' }] },
      timing,
    ),
    { ...timing, startMs: 1200, text: 'other text', style: 'Other' },
  );
});
test('retry refuses each competing edited field and never treats identical desired content as proof of own commit', () => {
  for (const field of ['text', 'style', 'startMs', 'endMs'] as const) {
    const value = field === 'text' || field === 'style' ? 'changed' : 1500;
    const draft = { ...edit, [field]: value } as CueEdit;
    const current = {
      ...base,
      revision: 2,
      cues: [{ ...base.cues[0]!, [field]: value }],
    };
    assert.equal(retryEdit(base, current, draft), null, field);
  }
});
test('retry requires live same-generation cue identity', () => {
  assert.equal(retryEdit(base, { ...base, generation: 2 }, edit), null);
  assert.equal(retryEdit(base, { ...base, loaded: false }, edit), null);
  assert.equal(retryEdit(base, { ...base, cues: [] }, edit), null);
  assert.equal(
    retryEdit(base, { ...base, cues: [{ ...base.cues[0]!, id: 8 }] }, edit),
    null,
  );
});
