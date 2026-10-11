import type { updateMenu } from '../../../src/lib/client';
import { defaultPreferences } from '../../../src/lib/client';
import { mount, tick, unmount } from 'svelte';
import { isTauri } from '@tauri-apps/api/core';
import App from '../../../src/App.svelte';
import {
  emptyDocument,
  emptyStatus,
  type DocumentAction,
  type DocumentSnapshot,
  type MediaAction,
} from '../../../src/lib/client';

// Browser-only integration fixture: real App/client, modeled IPC, no disk writes.
if (isTauri()) throw new Error('Run this fixture in an ordinary browser.');
const host = document.getElementById('test')!;
const heading = document.getElementById('result')!;
const output = document.getElementById('checks')!;
let snapshot: DocumentSnapshot = structuredClone(emptyDocument);
let destination: string | null = null;
let failSave = false;
let pickers = 0;
let saves = 0;
const media = {
  ...emptyStatus,
  available: true,
  loaded: true,
  session: 1,
  durationSeconds: 100,
  positionSeconds: 5,
};
const transport: string[] = [];
function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}
function passed(message: string) {
  const item = document.createElement('li');
  item.textContent = `PASS: ${message}`;
  output.append(item);
}
const callbacks = new Map<number, (value: unknown) => void>();
let eventHandler = 0;
let menuState: Parameters<typeof updateMenu>[0] | undefined;
function menuAction(action: string) {
  callbacks.get(eventHandler)!({ payload: action });
}
function button(label: string) {
  const found = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!found) throw new Error(`Missing ${label}`);
  return found;
}
async function until(condition: () => boolean) {
  const deadline = performance.now() + 5000;
  while (!condition()) {
    if (performance.now() > deadline) throw new Error('UI response timeout');
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  await tick();
}
Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
  configurable: true,
  value: { unregisterListener: () => {} },
});
Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    transformCallback: (callback: (value: unknown) => void) => {
      const id = callbacks.size + 1;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback: () => {},
    async invoke(
      command: string,
      args: {
        action?: DocumentAction | MediaAction;
        kind?: string;
        handler?: number;
        state?: Parameters<typeof updateMenu>[0];
      },
    ) {
      if (command === 'preferences_get')
        return structuredClone(defaultPreferences);
      switch (command) {
        case 'plugin:event|listen':
          eventHandler = args.handler!;
          return 1;
        case 'plugin:event|unlisten':
          return;
        case 'update_menu':
          menuState = args.state;
          return;
        case 'recovery_status':
          return { entries: [], error: null };
        case 'media_action':
          if (args.action?.kind !== 'status' && args.action?.kind !== 'layout')
            transport.push(args.action!.kind);
          return structuredClone(media);
        case 'choose_file':
          assert(args.kind === 'saveSubtitles', 'Unexpected picker');
          pickers++;
          return destination;
        case 'document_action': {
          const action = args.action as DocumentAction;
          if (action.kind === 'create') {
            assert(
              !snapshot.loaded && action.afterId === null,
              'Expected first cue',
            );
            snapshot = {
              ...snapshot,
              generation: 1,
              revision: 1,
              loaded: true,
              dirty: true,
              requiresSaveAs: true,
              canUndo: true,
              cues: [
                {
                  id: 1,
                  startMs: action.startMs,
                  endMs: action.endMs,
                  text: action.text,
                },
              ],
            };
          } else if (action.kind === 'save') {
            saves++;
            if (failSave) throw new Error('Disposable save failure');
            snapshot = {
              ...snapshot,
              path: action.path ?? snapshot.path,
              dirty: false,
              requiresSaveAs: false,
            };
          } else
            assert(
              action.kind === 'status',
              `Unexpected document action: ${action.kind}`,
            );
          return structuredClone(snapshot);
        }
        default:
          throw new Error(`Unexpected IPC: ${command}`);
      }
    },
  },
});
async function run() {
  const app = mount(App, { target: host });
  try {
    await until(
      () =>
        !(!menuState || menuState.busy) &&
        !(!menuState || menuState.busy || !menuState.available),
    );
    button('Create cue').click();
    await until(
      () =>
        snapshot.loaded &&
        !(!menuState || menuState.busy || !menuState.document),
    );
    assert(
      snapshot.cues[0]?.startMs === 5000 && snapshot.cues[0]?.endMs === 7000,
      'First cue did not use playhead',
    );
    assert(
      !!host.textContent?.includes('Untitled subtitles'),
      'Untitled label missing',
    );
    passed('media without SRT can create an untitled cue at the playhead');
    menuAction('save');
    await until(
      () =>
        pickers === 1 && !(!menuState || menuState.busy || !menuState.document),
    );
    assert(
      saves === 0 && snapshot.dirty && snapshot.path === null,
      'Cancelled save changed document',
    );
    passed(
      'first Save opens destination picker; cancellation preserves unsaved cues',
    );
    destination = '/disposable/new.srt';
    failSave = true;
    menuAction('save');
    await until(
      () =>
        saves === 1 && !(!menuState || menuState.busy || !menuState.document),
    );
    assert(
      snapshot.path === null &&
        snapshot.dirty &&
        !!host.textContent?.includes('Disposable save failure'),
      'Failed save lost state/error',
    );
    passed('failed first save retains untitled state and reports failure');
    failSave = false;
    menuAction('save');
    await until(
      () =>
        saves === 2 && !(!menuState || menuState.busy || !menuState.document),
    );
    assert(
      snapshot.path === destination && !snapshot.dirty && pickers === 3,
      'First save did not adopt path',
    );
    menuAction('save');
    await until(
      () =>
        saves === 3 && !(!menuState || menuState.busy || !menuState.document),
    );
    assert(pickers === 3, 'Later save requested a second destination');
    passed('successful first save adopts its path; later Save reuses it');
    const canvas = host.querySelector('canvas')!;
    const checkbox = [...host.querySelectorAll('label')]
      .find((l) => l.textContent?.trim() === 'Follow playback')!
      .querySelector('input')!;
    const pan = host.querySelector<HTMLInputElement>('.pan input')!;
    canvas.dispatchEvent(
      new WheelEvent('wheel', {
        deltaX: 3000,
        clientX: canvas.getBoundingClientRect().left + 200,
        bubbles: true,
        cancelable: true,
      }),
    );
    await until(() => !checkbox.checked && Number(pan.value) > 5);
    assert(transport.length === 0, 'Navigation moved media');
    passed('horizontal wheel pans independently and unchecks Follow playback');
    checkbox.click();
    await until(() => checkbox.checked && Number(pan.value) === 0);
    assert(transport.length === 0, 'Follow checkbox sought the video');
    passed(
      'Follow playback checkbox restores the playhead viewport without seeking',
    );
    heading.textContent = 'PASS: all 6 new-subtitle/navigation checks';
  } catch (error) {
    heading.textContent = `FAIL: ${String(error)}`;
  } finally {
    await unmount(app);
    Reflect.deleteProperty(window, '__TAURI_INTERNALS__');
    Reflect.deleteProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__');
    Reflect.deleteProperty(window, 'isTauri');
  }
}
void run();
