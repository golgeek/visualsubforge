import test from 'node:test';
import assert from 'node:assert/strict';
import { workspaceActions } from '../../../tests/frontend/ipc-contract-samples.ts';
import {
  actions,
  actionDescription,
  unavailable,
  shortcutLabel,
  type ActionContext,
} from './action-registry.ts';

const empty: ActionContext = {
  ready: true,
  busy: false,
  document: false,
  media: false,
  available: false,
  cue: false,
  cues: false,
  draft: false,
  styled: false,
  webvtt: false,
  undo: false,
  redo: false,
  range: false,
  scenesRunning: false,
  scenesPending: false,
  previousScene: false,
  nextScene: false,
  problems: false,
  previousProblem: false,
  nextProblem: false,
  operations: {
    createCue: 'Select a time range',
    deleteCue: 'Select a cue',
    splitCue: 'Select a cue',
    mergePrevious: 'Select a cue',
    mergeNext: 'Select a cue',
  },
};
test('registry uniquely covers every serialized Rust workspace action', () => {
  assert.equal(new Set(actions.map((a) => a.id)).size, actions.length);
  assert.deepEqual(
    actions
      .filter((a) => a.native)
      .map((a) => a.id)
      .sort(),
    [...workspaceActions].sort(),
  );
  for (const action of actions) {
    assert.ok(action.label && action.group);
    assert.equal(
      unavailable(action.id, { ...empty, ready: false }),
      'Workspace is starting',
    );
    assert.equal(
      unavailable(action.id, { ...empty, busy: true }),
      'Finish the current operation',
    );
  }
});
test('unavailable actions explain the missing condition and become available', () => {
  for (const id of [
    'openMedia',
    'play',
    'playRange',
    'save',
    'saveProject',
    'apply',
    'revertDraft',
    'undo',
    'redo',
    'convertSubtitles',
    'fixGaps',
    'fixReadingSpeed',
    'snapShots',
    'resync',
    'styles',
    'fonts',
    'previousScene',
    'nextProblem',
    'createCue',
    'deleteCue',
  ] as const)
    assert.ok(unavailable(id, empty), id);
  assert.equal(unavailable('play', { ...empty, media: true }), null);
  assert.equal(
    unavailable('snapShots', { ...empty, cues: true }),
    null,
    'review can explain missing scene coverage before preparation',
  );
  assert.equal(
    unavailable('convertSubtitles', { ...empty, document: true, webvtt: true }),
    'WebVTT export is not available',
  );
  assert.equal(
    unavailable('splitCue', { ...empty, operations: { splitCue: null } }),
    null,
  );
  assert.equal(
    unavailable('cancelSceneScan', { ...empty, scenesRunning: true }),
    null,
  );
});
test('bindings are unique per native or webview routing surface; the pre-existing Mac collision stays explicit', () => {
  for (const mac of [false, true]) {
    const bindings = new Map<string, string[]>();
    for (const action of actions) {
      for (const shortcut of [
        mac ? (action.macShortcut ?? action.shortcut) : action.shortcut,
        ...action.alternateShortcuts,
      ].filter(Boolean)) {
        const key = shortcutLabel(shortcut, mac);
        bindings.set(key, [...(bindings.get(key) ?? []), action.id]);
      }
    }
    const collisions = [...bindings].filter(([, ids]) => ids.length > 1);
    // Native Merge and the webview's Replace already share this Mac chord.
    // 026c preserves defaults; 026h must resolve the cross-surface preset choice.
    assert.deepEqual(
      collisions,
      mac ? [['⌘⇧H', ['mergePrevious', 'replace']]] : [],
    );
  }
});
test('legacy tool aliases remain searchable and platform glyphs agree', () => {
  assert.ok(
    actionDescription('fixGaps').aliases.includes('Enforce minimum gaps'),
  );
  assert.ok(
    actionDescription('convertSubtitles').aliases.includes('Convert subtitles'),
  );
  assert.equal(shortcutLabel(actionDescription('save').shortcut, true), '⌘S');
  assert.equal(
    shortcutLabel(actionDescription('save').shortcut, false),
    'Ctrl+S',
  );
});
