import test from 'node:test';
import assert from 'node:assert/strict';
import { routeWorkspaceKey } from './workspace-keys.ts';

type Key = Parameters<typeof routeWorkspaceKey>[0];
type Context = Parameters<typeof routeWorkspaceKey>[1];
const context: Context = {
  editing: false,
  control: false,
  menu: false,
  confirming: false,
  problems: false,
  windows: false,
  available: true,
  canAct: true,
  timing: false,
  paused: true,
};
function route(
  key: string,
  modifiers: Partial<Key> = {},
  state: Partial<Context> = {},
) {
  return routeWorkspaceKey(
    {
      key,
      code: key === ' ' ? 'Space' : key,
      ctrlKey: false,
      metaKey: false,
      altKey: false,
      shiftKey: false,
      repeat: false,
      isComposing: false,
      defaultPrevented: false,
      ...modifiers,
    },
    { ...context, ...state },
  );
}

test('find and replace precede Windows merge shortcuts, including inside text', () => {
  for (const modifier of ['ctrlKey', 'metaKey'])
    assert.deepEqual(
      route('f', { [modifier]: true }, { editing: true, windows: true }),
      { kind: 'text', mode: 'find' },
    );
  assert.deepEqual(route('h', { ctrlKey: true }, { windows: true }), {
    kind: 'text',
    mode: 'replace',
  });
  assert.deepEqual(route('h', { metaKey: true, shiftKey: true }), {
    kind: 'text',
    mode: 'replace',
  });
  assert.deepEqual(
    route('h', { ctrlKey: true, shiftKey: true }, { windows: true }),
    { kind: 'action', action: 'mergePrevious' },
  );
  assert.equal(route('h', { metaKey: true }), null);
});

test('composition, consumed keys and modal/menu presentation leave keys alone', () => {
  for (const modifiers of [{ isComposing: true }, { defaultPrevented: true }])
    assert.equal(route('f', { ctrlKey: true, ...modifiers }), null);
  for (const state of [{ menu: true }, { confirming: true }]) {
    assert.equal(route(' ', {}, state), null);
    assert.equal(route('Escape', {}, state), null);
    assert.equal(route('f', { ctrlKey: true }, state), null);
  }
  assert.equal(route('f', { ctrlKey: true, repeat: true }), null);
  assert.equal(
    route('f', { ctrlKey: true, altKey: true }, { windows: true }),
    null,
  );
});

test('Problems F4 routes direction and permits repeat only with the panel open', () => {
  assert.deepEqual(route('F4', {}, { problems: true }), {
    kind: 'problem',
    previous: false,
  });
  assert.deepEqual(
    route('F4', { shiftKey: true, repeat: true }, { problems: true }),
    { kind: 'problem', previous: true },
  );
  assert.equal(route('F4'), null);
  for (const modifier of ['ctrlKey', 'metaKey', 'altKey'])
    assert.equal(route('F4', { [modifier]: true }, { problems: true }), null);
});

test('Enter respects text and controls, while Escape routes back to the waveform', () => {
  assert.deepEqual(route('Enter'), { kind: 'editing', action: 'focusText' });
  assert.equal(route('Enter', {}, { editing: true }), null);
  assert.equal(route('Enter', {}, { control: true }), null);
  assert.deepEqual(route('Escape', {}, { editing: true }), {
    kind: 'editing',
    action: 'focusWaveform',
  });
  assert.deepEqual(route('Escape', {}, { control: true }), {
    kind: 'editing',
    action: 'focusWaveform',
  });
});

test('editing shortcuts route while typing without stealing AltGr', () => {
  const cases = [
    ['ArrowLeft', false, 'seekBack'],
    ['ArrowRight', true, 'seekForwardFar'],
    ['ArrowUp', false, 'previousCue'],
    ['ArrowDown', false, 'nextCue'],
    [' ', false, 'play'],
  ] as const;
  for (const [key, shiftKey, action] of cases)
    assert.deepEqual(
      route(key, { altKey: true, shiftKey }, { editing: true }),
      { kind: 'editing', action },
    );
  assert.equal(
    route(
      'ArrowLeft',
      { altKey: true, ctrlKey: true },
      { editing: true, windows: true },
    ),
    null,
  );
});

test('Windows shortcuts dispatch workspace actions and consume unavailable media open', () => {
  assert.deepEqual(
    route('s', { ctrlKey: true }, { windows: true, editing: true }),
    { kind: 'action', action: 'save' },
  );
  assert.deepEqual(
    route(
      'Enter',
      { ctrlKey: true, altKey: true },
      { windows: true, editing: true },
    ),
    { kind: 'action', action: 'play' },
  );
  assert.deepEqual(
    route('o', { ctrlKey: true }, { windows: true, available: false }),
    { kind: 'consume' },
  );
  assert.deepEqual(
    route(
      'o',
      { ctrlKey: true, shiftKey: true },
      { windows: true, available: false },
    ),
    { kind: 'action', action: 'openSubtitles' },
  );
  assert.equal(
    route('z', { ctrlKey: true }, { windows: true, editing: true }),
    null,
  );
  assert.equal(route('s', { ctrlKey: true }), null);
});

test('plain Space starts held timing only when playing in timing mode', () => {
  assert.deepEqual(route(' '), { kind: 'action', action: 'play' });
  assert.deepEqual(route(' ', {}, { timing: true, paused: true }), {
    kind: 'action',
    action: 'play',
  });
  assert.deepEqual(route(' ', {}, { timing: true, paused: false }), {
    kind: 'held',
  });
  assert.deepEqual(
    route(' ', { repeat: true }, { timing: true, paused: false }),
    { kind: 'consume' },
  );
  assert.deepEqual(route(' ', {}, { canAct: false }), { kind: 'consume' });
  assert.equal(route(' ', {}, { editing: true }), null);
  assert.equal(route(' ', {}, { control: true }), null);
  assert.equal(route(' ', { shiftKey: true }), null);
});
