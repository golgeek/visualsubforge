import { emptyQuality } from '../empty-quality';
import { mount, tick, unmount } from 'svelte';
import App from '../../../src/App.svelte';
import {
  actionDescription,
  shortcutLabel,
} from '../../../src/lib/workspace/action-registry';
import {
  defaultPreferences,
  emptyDocument,
  emptyStatus,
  type MediaAction,
  type DocumentAction,
  type Preferences,
} from '../../../src/lib/client';

// Production workspace, modeled IPC. Visibility follows accepted media status;
// no requests or document ownership are replaced by UI-only action flags.
const host = document.querySelector<HTMLElement>('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
const media = { ...emptyStatus, available: true, surfaceAboveWebview: true };
const snapshot = {
  ...emptyDocument,
  loaded: true,
  generation: 1,
  cues: [
    { id: 1, startMs: 1000, endMs: 2000, text: 'A subtitle' },
    { id: 2, startMs: 3000, endMs: 4000, text: 'Green apples' },
  ],
};
const callbacks = new Map<number, (value: unknown) => void>();
let listener = 0;
let occluded = false;
let preferences = structuredClone(defaultPreferences);
let settingsEpoch = 1;
let creations = 0;
function assert(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const li = document.createElement('li');
  li.textContent = `PASS: ${message}`;
  output.append(li);
}
function button(label: string) {
  return [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
}
async function until(predicate: () => boolean) {
  const end = performance.now() + 5000;
  while (!predicate()) {
    assert(performance.now() < end, 'Timed out: ' + host.textContent);
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  await tick();
}
Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
  configurable: true,
  value: { unregisterListener: () => {} },
});
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    transformCallback(callback: (value: unknown) => void) {
      const id = callbacks.size + 1;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback() {},
    async invoke(
      command: string,
      args: {
        action?: MediaAction | DocumentAction;
        handler?: number;
        preferences?: Preferences;
      },
    ) {
      switch (command) {
        case 'quality_read':
          return emptyQuality(args);
        case 'quality_cancel':
          return null;
        case 'plugin:event|listen':
          listener = args.handler!;
          return 1;
        case 'plugin:event|unlisten':
        case 'update_menu':
          return;
        case 'preferences_get':
          return structuredClone(preferences);
        case 'preferences_save':
          preferences = structuredClone(args.preferences!);
          return ++settingsEpoch;
        case 'document_action':
          if (args.action?.kind === 'create') {
            creations++;
            snapshot.cues.push({
              id: 2 + creations,
              startMs: args.action.startMs,
              endMs: args.action.endMs,
              text: args.action.text,
            });
            snapshot.revision++;
          }
          return structuredClone(snapshot);
        case 'media_action':
          if (args.action?.kind === 'occlude') occluded = args.action.occluded;
          return structuredClone(media);
        case 'recovery_status':
          return { entries: [], error: null };
        case 'quality_settings':
          return {
            current: {
              epoch: settingsEpoch,
              profile: preferences.quality,
              timing: preferences.timing,
            },
            builtins: [defaultPreferences.quality],
          };
        case 'language_words':
          return {
            epoch: settingsEpoch,
            personal: { englishUs: [], french: [] },
            ignored: { englishUs: [], french: [] },
          };
        default:
          throw new Error('Unexpected test command: ' + command);
      }
    },
  },
});

const browser = () =>
  host.querySelector<HTMLDialogElement>(
    'dialog[aria-labelledby="action-browser-title"][open]',
  );
function key(
  target: EventTarget,
  value: string,
  modifiers: KeyboardEventInit = {},
) {
  target.dispatchEvent(
    new KeyboardEvent('keydown', {
      key: value,
      bubbles: true,
      cancelable: true,
      ...modifiers,
    }),
  );
}
async function search(value: string) {
  const field = browser()!.querySelector<HTMLInputElement>('input')!;
  field.value = value;
  field.dispatchEvent(new Event('input', { bubbles: true }));
  await tick();
}
async function close() {
  browser()!.dispatchEvent(new Event('cancel', { cancelable: true }));
  await until(() => !browser() && !occluded);
}
async function run() {
  const app = mount(App, { target: host });
  try {
    await until(
      () => !!listener && host.querySelectorAll('.cue-row').length === 2,
    );
    const editor = host.querySelector<HTMLTextAreaElement>(
      '[aria-label="Subtitle text"]',
    )!;
    editor.focus();
    key(editor, '?', { shiftKey: true });
    await tick();
    assert(!browser(), 'Question mark stays text inside the editor');
    key(editor, 'k', { ctrlKey: true });
    await until(() => !!browser());
    assert(occluded, 'Palette hides native video before showing');
    assert(
      browser()!.querySelector('input') === document.activeElement,
      'Palette focuses search',
    );
    await search('Play / Pause');
    const play = browser()!.querySelector<HTMLButtonElement>(
      '[data-action="play"]',
    )!;
    assert(
      play.disabled && play.textContent!.includes('Open media first'),
      `Unavailable command explains why: disabled=${play.disabled}, text=${play.textContent}`,
    );
    await search('Enforce minimum gaps');
    assert(
      browser()!.querySelector('[data-action="fixGaps"]'),
      'Legacy tool alias finds renamed action',
    );
    await close();
    await until(() => document.activeElement === editor);
    passed(
      'Palette opens while typing, filters aliases and reasons, and Escape restores focus/video',
    );
    key(editor, 'k', { ctrlKey: true });
    await until(() => !!browser());
    await search('#2');
    const cue = browser()!.querySelector<HTMLButtonElement>('[data-result]')!;
    assert(
      cue.textContent!.includes('Green apples'),
      'Cue number resolves accepted cue',
    );
    cue.click();
    await until(() => !browser() && editor.value === 'Green apples');
    key(editor, 'k', { ctrlKey: true });
    await until(() => !!browser());
    await search('subtitle');
    assert(
      browser()!.textContent!.includes('#1 · A subtitle'),
      'Cue text is searchable',
    );
    await search('Edit selected cue');
    browser()!
      .querySelector<HTMLButtonElement>('[data-action="focusText"]')!
      .click();
    await until(
      () => !browser() && document.activeElement === editor && !occluded,
    );
    passed(
      'Palette jumps by cue number, searches text and executes an existing focus action',
    );
    const commands = button('Commands')!;
    commands.focus();
    key(commands, '?', { shiftKey: true });
    await until(() => !!browser());
    assert(
      browser()!.textContent!.includes('Keyboard shortcuts'),
      'Question mark opens the sheet outside text',
    );
    await search('Split at Playhead');
    const label = actionDescription('splitCue');
    const chord = shortcutLabel(
      label.shortcut,
      navigator.platform.startsWith('Mac'),
    );
    assert(
      browser()!.textContent!.includes(label.label) &&
        browser()!.textContent!.includes(chord),
      'Sheet uses shared label and shortcut',
    );
    await close();
    commands.click();
    await until(() => !!browser());
    await search('Split at Playhead');
    assert(
      browser()!.textContent!.includes(label.label) &&
        browser()!.textContent!.includes(chord),
      'Command menu uses shared label and shortcut',
    );
    await close();
    const anchor = host.querySelector<HTMLElement>(
      '[data-tooltip-action="splitCue"]',
    )!;
    anchor.dispatchEvent(new PointerEvent('pointerenter'));
    await until(() => !!host.querySelector('[role="tooltip"]'));
    assert(occluded, 'Tooltip also hides native video');
    const tooltip = host.querySelector('[role="tooltip"]')!;
    assert(
      tooltip.textContent!.includes(label.label) &&
        tooltip.textContent!.includes(chord),
      'Tooltip uses shared label and shortcut',
    );
    anchor.dispatchEvent(new PointerEvent('pointerleave'));
    await until(() => !host.querySelector('[role="tooltip"]') && !occluded);
    passed(
      'Searchable shortcut sheet, command menu and delayed tooltip share metadata and safe video restoration',
    );
    callbacks.get(listener)!({ payload: 'settings' });
    await until(() => !!host.querySelector('[aria-label="Shortcut preset"]'));
    button('Shortcuts')!.click();
    const preset = host.querySelector<HTMLSelectElement>(
      '[aria-label="Shortcut preset"]',
    )!;
    await until(() => !preset.disabled);
    preset.value = 'vsfDefault';
    preset.dispatchEvent(new Event('change', { bubbles: true }));
    await until(() => preferences.shortcuts.preset === 'vsfDefault');
    button('Close')!.click();
    await until(() => !host.querySelector('dialog[open]') && !occluded);
    editor.focus();
    key(editor, 'n');
    await tick();
    assert(creations === 0, 'Single-letter command stays ordinary cue text');
    const row = host.querySelector<HTMLButtonElement>('.cue-row')!;
    row.focus();
    key(row, 'n', { isComposing: true });
    await tick();
    assert(creations === 0, 'Composition cannot dispatch a list command');
    key(row, 'n');
    await until(() => creations === 1);
    await until(() => document.activeElement === editor);
    commands.focus();
    key(commands, 'n');
    await tick();
    assert(
      Number(creations) === 1,
      'Other buttons retain ordinary control focus',
    );
    passed(
      'Confirmed trial preset routes cue-row keys through Rust while protecting text, composition and other controls',
    );
    heading.textContent = `PASS: all ${output.childElementCount} action discovery checks`;
  } catch (error) {
    heading.textContent = 'FAIL: ' + String(error);
  } finally {
    await unmount(app);
    for (const name of [
      'isTauri',
      '__TAURI_INTERNALS__',
      '__TAURI_EVENT_PLUGIN_INTERNALS__',
    ])
      Reflect.deleteProperty(window, name);
  }
}
void run();
