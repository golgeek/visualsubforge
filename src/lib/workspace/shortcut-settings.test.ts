import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import {
  bindingAction,
  canonicalShortcut,
  shortcutError,
  shortcutFromEvent,
  shortcutPreset,
} from './shortcut-settings.ts';
const key = (key: string, shiftKey = false) => ({
  key,
  shiftKey,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
});
test('presets preserve focus separation and current settings remain opt-in', () => {
  for (const preset of [
    'current',
    'vsfDefault',
    'visualSubSyncClassic',
  ] as const)
    assert.equal(shortcutError(shortcutPreset(preset)), null);
  const current = shortcutPreset('current');
  const proposed = shortcutPreset('vsfDefault');
  assert.equal(bindingAction(current, key('n'), 'workspace'), null);
  assert.equal(bindingAction(proposed, key('n'), 'workspace'), 'createCue');
  assert.equal(bindingAction(proposed, key('n'), 'text'), null);
  assert.equal(bindingAction(proposed, key('n'), 'control'), null);
  assert.equal(bindingAction(proposed, key('Tab'), 'text'), 'play');
  assert.equal(bindingAction(proposed, key('Tab'), 'control'), null);
  assert.equal(bindingAction(proposed, key('Tab', true), 'text'), 'replay');
  assert.equal(
    bindingAction(shortcutPreset('visualSubSyncClassic'), key('F1'), 'text'),
    'play',
  );
});
test('conflicts include global/text intersections and Enter aliases', () => {
  const settings = shortcutPreset('current');
  settings.bindings.push({
    action: 'splitCue',
    mode: 'text',
    key: 'CmdOrCtrl+S',
  });
  assert.match(shortcutError(settings)!, /conflicts/);
  settings.bindings.at(-1)!.key = 'Shift+Tab';
  assert.equal(shortcutError(settings), null);
  settings.bindings.push({
    action: 'createCue',
    mode: 'workspace',
    key: 'Return',
  });
  assert.match(shortcutError(settings)!, /conflicts/);
  assert.equal(canonicalShortcut('Alt+Enter'), canonicalShortcut('Alt+Return'));
});
test('reserved typing and OS chords cannot be stolen', () => {
  for (const value of [
    'CmdOrCtrl+C',
    'Alt+Tab',
    'Escape',
    'CmdOrCtrl+B',
    'CmdOrCtrl+Q',
    'X',
    'Shift+CmdOrCtrl+S',
  ]) {
    const settings = shortcutPreset('current');
    settings.bindings.push({ action: 'splitCue', mode: 'text', key: value });
    assert.notEqual(shortcutError(settings), null, value);
  }
});
test('recorded keys are canonical and editing a preset does not mutate its source', () => {
  assert.equal(
    shortcutFromEvent({ ...key('s', true), metaKey: true }),
    'CmdOrCtrl+Shift+S',
  );
  assert.equal(shortcutFromEvent(key('?', true)), '?');
  assert.equal(shortcutFromEvent(key('Meta')), null);
  const settings = shortcutPreset('current');
  settings.bindings[0]!.key = '';
  assert.notEqual(shortcutPreset('current').bindings[0]!.key, '');
});
