import { test } from 'node:test';
import assert from 'node:assert/strict';
import { windowsShortcut } from './shortcuts.ts';

const key = {
  key: 'o',
  ctrlKey: true,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  repeat: false,
  isComposing: false,
  defaultPrevented: false,
};

test('Windows file and transport commands distinguish modifiers', () => {
  assert.equal(windowsShortcut(key, false), 'openMedia');
  assert.equal(
    windowsShortcut({ ...key, key: 'O', shiftKey: true }, true),
    'openSubtitles',
  );
  assert.equal(windowsShortcut({ ...key, key: 's' }, true), 'save');
  assert.equal(
    windowsShortcut({ ...key, key: 'S', shiftKey: true }, true),
    'saveAs',
  );
  assert.equal(windowsShortcut({ ...key, key: 'Enter' }, true), 'apply');
  assert.equal(
    windowsShortcut({ ...key, key: 'Enter', altKey: true }, true),
    'play',
  );
  assert.equal(
    windowsShortcut({ ...key, key: 'Enter', shiftKey: true }, true),
    null,
  );
});

test('Text history remains local and unmodified input is not intercepted', () => {
  for (const shiftKey of [false, true]) {
    const undo = { ...key, key: 'z', shiftKey };
    assert.equal(windowsShortcut(undo, true), null);
    assert.equal(windowsShortcut(undo, false), shiftKey ? 'redo' : 'undo');
  }
  for (const changed of [
    { ctrlKey: false },
    { metaKey: true },
    { altKey: true },
    { repeat: true },
    { isComposing: true },
    { defaultPrevented: true },
    { key: 'a' },
    { key: 'c' },
    { key: 'v' },
  ])
    assert.equal(windowsShortcut({ ...key, ...changed }, false), null);
});

test('Cue shortcuts are explicit modified commands, never ordinary text deletion', () => {
  const commands = {
    n: 'createCue',
    d: 'deleteCue',
    k: 'splitCue',
    h: 'mergePrevious',
    j: 'mergeNext',
  };
  for (const [letter, action] of Object.entries(commands)) {
    assert.equal(
      windowsShortcut({ ...key, key: letter, shiftKey: true }, true),
      action,
    );
    assert.equal(windowsShortcut({ ...key, key: letter }, true), null);
    assert.equal(
      windowsShortcut(
        { ...key, key: letter, shiftKey: true, altKey: true },
        true,
      ),
      null,
    );
  }
  assert.equal(
    windowsShortcut({ ...key, key: 'Delete', ctrlKey: false }, false),
    null,
  );
  assert.equal(windowsShortcut({ ...key, key: 'Backspace' }, true), null);
});

test('Editing transport shortcuts work in text, while ordinary typing and AltGr stay local', async () => {
  const { editingShortcut } = await import('./shortcuts.ts');
  const alt = { ...key, ctrlKey: false, altKey: true };
  for (const editing of [false, true]) {
    assert.equal(editingShortcut({ ...alt, key: ' ' }, editing), 'play');
    assert.equal(
      editingShortcut({ ...alt, key: 'ArrowLeft' }, editing),
      'seekBack',
    );
    assert.equal(
      editingShortcut({ ...alt, key: 'ArrowRight', shiftKey: true }, editing),
      'seekForwardFar',
    );
    assert.equal(
      editingShortcut({ ...alt, key: 'ArrowDown' }, editing),
      'nextCue',
    );
    assert.equal(
      editingShortcut({ ...alt, key: 'Escape', altKey: false }, editing),
      'focusWaveform',
    );
    assert.equal(
      editingShortcut({ ...alt, key: 'ArrowLeft', ctrlKey: true }, editing),
      null,
    );
    assert.equal(
      editingShortcut({ ...alt, key: ' ', isComposing: true }, editing),
      null,
    );
    assert.equal(
      editingShortcut({ ...alt, key: ' ', repeat: true }, editing),
      null,
    );
  }
  assert.equal(
    editingShortcut({ ...alt, key: ' ', altKey: false }, true),
    null,
  );
  assert.equal(
    editingShortcut({ ...alt, key: 'Enter', altKey: false }, true),
    null,
  );
  assert.equal(
    editingShortcut({ ...alt, key: 'Enter', altKey: false }, false),
    'focusText',
  );
});
