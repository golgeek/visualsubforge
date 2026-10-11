import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  planCueOperation,
  operationSelection,
  reconcileCueSelection,
  cueOperationLabels,
  type OperationContext,
} from './cue-operations.ts';

const cues = [
  { id: 8, startMs: 3000, endMs: 5000, text: '日本語' },
  { id: 3, startMs: 1000, endMs: 4000, text: 'B' },
  { id: 12, startMs: 9000, endMs: 10000, text: '' },
] as const;
const context: OperationContext = {
  cues,
  selected: 8,
  loaded: true,
  draftDirty: false,
  positionSeconds: 4.0004,
  range: null,
};

test('Structural requests target stable IDs and round at the media boundary', () => {
  assert.deepEqual(planCueOperation('splitCue', context), {
    action: { kind: 'split', cueId: 8, atMs: 4000 },
    selection: 'created',
  });
  assert.deepEqual(planCueOperation('mergeNext', context), {
    action: { kind: 'merge', firstId: 8, secondId: 3 },
    selection: 8,
  });
  assert.deepEqual(
    planCueOperation('mergePrevious', { ...context, selected: 3 }),
    planCueOperation('mergeNext', context),
  );
  const created = planCueOperation('createCue', {
    ...context,
    range: [1.2345, 2.3456],
  });
  assert.deepEqual(created.action, {
    kind: 'create',
    afterId: 8,
    startMs: 1235,
    endMs: 2346,
    text: '',
  });
  assert.deepEqual(
    planCueOperation('createCue', { ...context, positionSeconds: null }).action,
    { kind: 'create', afterId: 8, startMs: 5000, endMs: 7000, text: '' },
  );
  assert.deepEqual(
    planCueOperation('createCue', {
      ...context,
      cues: [],
      selected: null,
      positionSeconds: null,
    }).action,
    { kind: 'create', afterId: null, startMs: 0, endMs: 2000, text: '' },
  );
});

test('Drafts, missing cues, split boundaries and invalid ranges never become requests', () => {
  for (const operation of Object.keys(
    cueOperationLabels,
  ) as (keyof typeof cueOperationLabels)[]) {
    assert.throws(
      () => planCueOperation(operation, { ...context, draftDirty: true }),
      /Apply or revert/,
    );
    if (operation !== 'createCue')
      assert.throws(
        () => planCueOperation(operation, { ...context, loaded: false }),
        /Open subtitles/,
      );
    if (operation !== 'createCue')
      assert.throws(
        () => planCueOperation(operation, { ...context, selected: 999 }),
        /Select a cue/,
      );
  }
  for (const positionSeconds of [null, 3, 5, 4.9999, NaN, Infinity, -1])
    assert.throws(() =>
      planCueOperation('splitCue', { ...context, positionSeconds }),
    );
  for (const range of [
    [1, 1.0001],
    [-1, 2],
    [2, 1],
    [0, Infinity],
  ] as const)
    assert.throws(() => planCueOperation('createCue', { ...context, range }));
  assert.throws(() => planCueOperation('mergePrevious', context));
  assert.throws(() =>
    planCueOperation('mergeNext', { ...context, selected: 12 }),
  );
});

test('Selection follows structural results, preserves surviving identities and handles an empty list', () => {
  assert.equal(planCueOperation('deleteCue', context).selection, 3);
  assert.equal(
    planCueOperation('deleteCue', { ...context, selected: 12 }).selection,
    3,
  );
  assert.equal(
    planCueOperation('deleteCue', { ...context, cues: [cues[0]] }).selection,
    null,
  );
  const added = { ...cues[0], id: 13 };
  const plan = planCueOperation('splitCue', context);
  assert.equal(
    operationSelection(plan, cues, [cues[0], added, ...cues.slice(1)]),
    13,
  );
  assert.equal(reconcileCueSelection(cues, [cues[0], cues[2]], 3), 12);
  assert.equal(reconcileCueSelection(cues, cues.slice(0, 2), 12), 3);
  assert.equal(reconcileCueSelection(cues, [added, ...cues], 3), 3);
  assert.equal(reconcileCueSelection(cues, [], 3), null);
  assert.equal(reconcileCueSelection([], cues, null), 8);
});

test('First cue starts untitled subtitles at the playhead or selected range', () => {
  const empty = { ...context, loaded: false, cues: [], selected: null };
  assert.deepEqual(planCueOperation('createCue', empty).action, {
    kind: 'create',
    afterId: null,
    startMs: 4000,
    endMs: 6000,
    text: '',
  });
  assert.deepEqual(
    planCueOperation('createCue', { ...empty, range: [2, 3.25] }).action,
    {
      kind: 'create',
      afterId: null,
      startMs: 2000,
      endMs: 3250,
      text: '',
    },
  );
});
