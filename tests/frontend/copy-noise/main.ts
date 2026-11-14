import { openCommands } from '../command-discovery';
import { mount, tick, unmount } from 'svelte';
import App from '../../../src/App.svelte';
import {
  defaultPreferences,
  emptyDocument,
  emptyStatus,
  type MediaAction,
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
  cues: [{ id: 1, startMs: 1000, endMs: 2000, text: 'A subtitle' }],
};
const callbacks = new Map<number, (value: unknown) => void>();
let listener = 0;
let occluded = false;
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
      args: { action?: MediaAction; handler?: number },
    ) {
      switch (command) {
        case 'plugin:event|listen':
          listener = args.handler!;
          return 1;
        case 'plugin:event|unlisten':
        case 'update_menu':
          return;
        case 'preferences_get':
          return structuredClone(defaultPreferences);
        case 'document_action':
          return structuredClone(snapshot);
        case 'media_action':
          if (args.action?.kind === 'occlude') occluded = args.action.occluded;
          return structuredClone(media);
        case 'recovery_status':
          return { entries: [], error: null };
        case 'quality_settings':
          return {
            current: {
              epoch: 1,
              profile: defaultPreferences.quality,
              timing: defaultPreferences.timing,
            },
            builtins: [defaultPreferences.quality],
          };
        case 'language_words':
          return {
            epoch: 1,
            personal: { englishUs: [], french: [] },
            ignored: { englishUs: [], french: [] },
          };
        default:
          throw new Error('Unexpected test command: ' + command);
      }
    },
  },
});
async function run() {
  const app = mount(App, { target: host });
  try {
    await until(
      () => !!listener && host.querySelectorAll('.cue-row').length === 1,
    );
    assert(
      !button('Start here') && !button('End here'),
      'Playhead timing actions hidden without media',
    );
    assert(
      button('Play') && !button('Pause'),
      'Unloaded transport does not say Pause',
    );
    assert(
      !/Desktop PoC|--features|vss_utf16_v1|ordinary_v1|UTF-16/.test(
        host.textContent ?? '',
      ),
      'Main window has no implementation copy',
    );
    passed(
      'Unavailable playhead actions are hidden and the empty transport says Play',
    );
    const menu = await openCommands(host);
    for (const label of [
      'Fix gaps and overlaps…',
      'Fix reading speed…',
      'Snap to shot changes…',
    ]) {
      assert(
        [...menu.querySelectorAll('button')].some((button) =>
          button.textContent?.includes(label),
        ),
        'Renamed command exists: ' + label,
      );
    }
    menu.dispatchEvent(new Event('cancel', { cancelable: true }));
    await until(
      () =>
        !host.querySelector('dialog[open]') && !button('Commands')!.disabled,
    );
    assert(button('Export As…'), 'Export name remains visible');
    passed('Tool discovery uses the subtitler-facing names');
    media.loaded = true;
    media.session = 1;
    media.positionSeconds = 1.5;
    media.paused = false;
    await until(
      () => !!button('Start here') && !!button('End here') && !!button('Pause'),
    );
    passed('Both playhead actions return when media becomes available');
    media.loaded = false;
    media.session = 2;
    await until(() => !button('Start here') && !button('End here'));
    passed('Both actions disappear again on media close');
    media.loaded = true;
    media.session = 3;
    await until(() => !!button('Start here'));
    callbacks.get(listener)!({ payload: 'settings' });
    await until(() => !!host.querySelector('dialog[open]'));
    assert(occluded, 'Settings hides video before presentation');
    assert(
      [...host.querySelectorAll('summary')].some(
        (node) => node.textContent === 'About this profile',
      ),
      'Counting disclosure remains reachable',
    );
    assert(
      host.querySelector('dialog[open]')!.textContent!.includes('UTF-16'),
      'Counting precision remains in Settings',
    );
    passed('Settings retains counting details behind the profile disclosure');
    heading.textContent = `PASS: all ${output.childElementCount} copy checks`;
  } catch (error) {
    heading.textContent = 'FAIL: ' + String(error);
  } finally {
    await unmount(app);
    for (const key of [
      'isTauri',
      '__TAURI_INTERNALS__',
      '__TAURI_EVENT_PLUGIN_INTERNALS__',
    ])
      Reflect.deleteProperty(window, key);
  }
}
void run();
