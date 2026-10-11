import type { updateMenu } from '../../../src/lib/client';
import { defaultPreferences } from '../../../src/lib/client';
import { mount, unmount, tick } from 'svelte';
import { isTauri } from '@tauri-apps/api/core';
import App from '../../../src/App.svelte';
import { defaultTiming } from '../../../src/lib/magnetic-timing';
import {
  emptyDocument,
  emptyStatus,
  type MediaAction,
  type DocumentAction,
  type ProjectData,
} from '../../../src/lib/client';
if (isTauri()) throw new Error('Browser fixture only');
const host = document.querySelector('#test')!;
let media = { ...emptyStatus, available: true };
let doc = structuredClone(emptyDocument);
let failOpen = false;
let failSettings = false;
let eventHandler = 0;
let savedSettings: unknown = null;
const edits: DocumentAction[] = [];
const loops: [number, number][] = [];
let exported: unknown = null;
const calls: string[] = [];
const saves: Record<string, unknown>[] = [];
const callbacks = new Map<number, (value: unknown) => void>();
const project: ProjectData = {
  version: 1,
  mediaPath: '/clip.mkv',
  subtitlePath: '/captions.srt',
  mediaIdentity: { size: 10, modified: '1', sampleHash: 'abc' },
  positionSeconds: 3,
  options: {
    waveformStart: 0,
    waveformSpan: 10,
    followPlayback: false,
    followSubtitle: false,
    showScenes: true,
    timing: { ...defaultTiming, enabled: true },
  },
  scenes: {
    version: 1,
    mode: 'visual',
    cuts: [4.5],
    identity: { size: 10, modified: '1', sampleHash: 'abc' },
  },
};
Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
  configurable: true,
  value: { unregisterListener: () => {} },
});
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
        action?: MediaAction | DocumentAction;
        handler?: number;
        state?: Parameters<typeof updateMenu>[0];
        request?: Record<string, unknown>;
        preset?: unknown;
        preferences?: unknown;
      },
    ) {
      calls.push(command);
      if (command === 'preferences_get')
        return structuredClone(defaultPreferences);
      switch (command) {
        case 'plugin:event|listen':
          eventHandler = args.handler!;
          return 1;
        case 'preferences_save':
          if (failSettings) throw new Error('Cannot save settings');
          savedSettings = args.preferences;
          return;
        case 'plugin:event|unlisten':
          return;
        case 'update_menu':
          menuState = args.state;
          return;
        case 'recovery_status':
          return { entries: [], error: null };
        case 'choose_file':
          return '/work.vsfproject';
        case 'project_open':
          if (failOpen) throw new Error('Project media has changed');
          return { ...structuredClone(project), token: 41 };
        case 'project_attach':
          return { path: '/work.vsfproject', token: 41 };
        case 'project_detach':
          return;
        case 'timing_preset_open':
          if (failOpen) throw new Error('Invalid timing preset');
          return {
            preset: {
              version: 1,
              name: 'Imported gap',
              timing: { ...defaultTiming, enabled: true, cueGapMs: 200 },
            },
            warnings: [],
          };
        case 'timing_preset_save':
          exported = args.preset;
          return;
        case 'project_save':
          saves.push(args.request!);
          return { path: args.request!.path, token: 42 };
        case 'scenes_restore':
          return 4;
        case 'scenes_view':
          return {
            generation: 4,
            mode: 'visual',
            scanning: null,
            running: false,
            error: null,
            total: 1,
            truncated: false,
            cuts: [4.5],
            previous: 1,
            next: 20,
          };
        case 'scenes_start':
          throw new Error('Restored project must not rescan');
        case 'media_action': {
          const action = args.action as MediaAction;
          if (action.kind === 'open')
            media = {
              ...media,
              session: media.session + 1,
              loaded: true,
              paused: true,
              durationSeconds: 100,
              positionSeconds: 0,
            };
          if (action.kind === 'loop') loops.push([action.start, action.end]);
          if (action.kind === 'seek') media.positionSeconds = action.seconds;
          return structuredClone(media);
        }
        case 'document_action': {
          const action = args.action as DocumentAction;
          if (action.kind === 'open')
            doc = {
              ...doc,
              loaded: true,
              path: '/captions.srt',
              generation: 1,
              cues: [
                { id: 1, startMs: 1000, endMs: 2000, text: 'Previous' },
                { id: 2, startMs: 3000, endMs: 4000, text: 'Edited' },
                { id: 3, startMs: 5000, endMs: 6000, text: 'Next' },
              ],
            };
          if (action.kind === 'edit') {
            edits.push(action);
            doc.cues = doc.cues.map((c) =>
              c.id === action.cueId
                ? { ...c, startMs: action.startMs, endMs: action.endMs }
                : c,
            );
            doc.revision++;
          }
          return structuredClone(doc);
        }
        case 'finish_exit':
          return;
        default:
          throw new Error(command);
      }
    },
  },
});
let menuState: Parameters<typeof updateMenu>[0] | undefined;
function menuAction(action: string) {
  callbacks.get(eventHandler)!({ payload: action });
}
function button(label: string) {
  const b = [...host.querySelectorAll('button')].find(
    (b) =>
      b.textContent?.trim() === label &&
      (!b.closest('dialog') || b.closest('dialog')!.open),
  );
  if (!b) throw new Error('Missing ' + label);
  return b;
}
async function until(check: () => boolean) {
  const deadline = performance.now() + 5000;
  await tick();
  while (!check()) {
    if (performance.now() > deadline)
      throw new Error('UI timeout: ' + host.textContent);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
function assert(ok: boolean, text: string) {
  if (!ok) throw new Error(text);
  const row = document.createElement('li');
  row.textContent = 'PASS: ' + text;
  document.querySelector('#checks')!.append(row);
}

function capture(element: HTMLElement) {
  let captured = false;
  element.setPointerCapture = () => {
    captured = true;
  };
  element.hasPointerCapture = () => captured;
  element.releasePointerCapture = () => {
    captured = false;
  };
}
function pointer(
  element: HTMLElement,
  type: string,
  x: number,
  altKey = false,
) {
  element.dispatchEvent(
    new PointerEvent(type, {
      pointerId: 1,
      isPrimary: true,
      button: 0,
      clientX: x,
      clientY: element.getBoundingClientRect().top + 40,
      altKey,
      bubbles: true,
      cancelable: true,
    }),
  );
}
function handle(boundary: string) {
  return host.querySelector<HTMLButtonElement>(
    `[aria-label^="Cue ${boundary} boundary"]`,
  )!;
}
async function drag(
  boundary: string,
  targetMs: number,
  finish: 'commit' | 'cancel' = 'commit',
  bypass = false,
) {
  const element = handle(boundary);
  capture(element);
  const original =
    boundary === 'start' ? doc.cues[1]!.startMs : doc.cues[1]!.endMs;
  const x = element.getBoundingClientRect().left + 5;
  const to =
    x +
    ((targetMs - original) / 10000) *
      host.querySelector('canvas')!.getBoundingClientRect().width;
  pointer(element, 'pointerdown', x);
  await tick();
  pointer(element, 'pointermove', to, bypass);
  await tick();
  await until(() => loops.length > 0);
  if (finish === 'commit') pointer(element, 'pointerup', to, bypass);
  else
    element.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Escape',
        bubbles: true,
        cancelable: true,
      }),
    );
  await until(() => !(!menuState || menuState.busy || !menuState.available));
}
async function run() {
  const app = mount(App, { target: host });
  try {
    await until(() => !(!menuState || menuState.busy || !menuState.available));
    menuAction('openProject');
    await until(() => !!host.textContent?.includes('Project restored'));
    await until(() => calls.includes('scenes_view'));
    assert(
      !host.querySelector('.settings'),
      'Timing configuration is absent from the editing workspace',
    );
    host.querySelectorAll<HTMLButtonElement>('.cue-row')[1]!.click();
    await until(
      () =>
        !!host
          .querySelector('.cue-row.selected')
          ?.textContent?.includes('Edited'),
    );
    button('Loop range').click();
    await until(
      () =>
        loops.length > 0 &&
        !(!menuState || menuState.busy || !menuState.available),
    );
    await drag('start', 2090);
    assert(
      doc.cues[1]!.startMs === 2080 &&
        edits.length === 1 &&
        loops.at(-1)?.[0] === 2.08,
      'Cue gap snapping commits once and updates the enabled loop',
    );
    const revision = doc.revision;
    await drag('end', 4380, 'cancel');
    assert(
      doc.cues[1]!.endMs === 4000 &&
        doc.revision === revision &&
        loops.at(-1)?.[1] === 4 &&
        loops.some((l) => l[1] === 4.37),
      'Scene-buffer preview snaps; Escape restores timing and loop without an edit',
    );
    await drag('end', 4380, 'commit', true);
    assert(
      doc.cues[1]!.endMs === 4380,
      'Alt-drag bypass keeps the exact pointer time',
    );
    // Keyboard steps remain precise even inside the magnetic radius.
    const end = handle('end');
    end.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'ArrowLeft',
        altKey: true,
        bubbles: true,
      }),
    );
    await tick();
    end.dispatchEvent(
      new KeyboardEvent('keyup', {
        key: 'ArrowLeft',
        altKey: true,
        bubbles: true,
      }),
    );
    await until(() => !(!menuState || menuState.busy || !menuState.available));
    assert(
      doc.cues[1]!.endMs === 4379,
      'Alt-arrow remains an exact one-millisecond nudge',
    );
    const canvas = host.querySelector('canvas')!;
    capture(canvas);
    const rect = canvas.getBoundingClientRect();
    const at = (ms: number) => rect.left + (rect.width * ms) / 10000;
    pointer(canvas, 'pointerdown', at(2010));
    pointer(canvas, 'pointermove', at(950));
    pointer(canvas, 'pointerup', at(950));
    await tick();
    assert(
      host.textContent?.includes('Range 0.950–2.010 s') === true,
      'A selection cannot magnetically land inside an existing cue or gap',
    );
    const beforeRange = doc.revision;
    pointer(canvas, 'pointerdown', at(200));
    pointer(canvas, 'pointermove', at(950));
    pointer(canvas, 'pointerup', at(950));
    await tick();
    assert(
      host.textContent?.includes('Range 0.200–0.920 s') === true &&
        doc.revision === beforeRange,
      'New range snaps to the cue gap without changing subtitle history',
    );
    pointer(canvas, 'pointerdown', at(950));
    pointer(canvas, 'pointermove', at(200));
    pointer(canvas, 'pointerup', at(200));
    await tick();
    assert(
      host.textContent?.includes('Range 0.200–0.920 s') === true,
      'Reverse range drag uses the same start/end policy',
    );
    pointer(canvas, 'pointerdown', at(950));
    pointer(canvas, 'pointermove', at(200), true);
    pointer(canvas, 'pointerup', at(200), true);
    await tick();
    assert(
      host.textContent?.includes('Range 0.200–0.950 s') === true,
      'Alt-drag bypass also applies to new range selection',
    );
    // Import/export are pure settings changes, not document transactions.
    callbacks.get(eventHandler)!({ payload: 'settings' });
    await until(() => !!host.querySelector('dialog[open] .settings'));
    assert(
      !!host.querySelector('dialog[open] .settings'),
      'Native settings action opens modal timing configuration',
    );
    const gap = [
      ...host.querySelectorAll<HTMLInputElement>(
        '.settings input[type="number"]',
      ),
    ][0]!;
    gap.value = '333';
    gap.dispatchEvent(new Event('change', { bubbles: true }));
    button('Cancel').click();
    await until(() => !host.querySelector('dialog[open]'));
    // Closing the HTML dialog precedes acknowledged native video/focus cleanup.
    await until(() => !menuState?.busy);
    callbacks.get(eventHandler)!({ payload: 'settings' });
    await until(() => !!host.querySelector('dialog[open] .settings'));
    assert(
      host.querySelector<HTMLInputElement>('.settings input[type="number"]')!
        .value === '80' && savedSettings === null,
      'Cancel discards settings drafts without writing preferences',
    );
    const beforeImport = doc.revision;
    button('Import timing preset').click();
    await until(() => !!host.textContent?.includes('Imported Imported gap'));
    button('Export timing preset').click();
    await until(
      () => exported !== null && !button('Export timing preset').disabled,
    );
    assert(
      (exported as { timing: { cueGapMs: number } }).timing.cueGapMs === 200 &&
        doc.revision === beforeImport,
      'Portable preset import/export changes policy without retiming cues',
    );
    failOpen = true;
    button('Import timing preset').click();
    await until(() => !!host.textContent?.includes('Invalid timing preset'));
    failSettings = true;
    button('Apply settings').click();
    await until(() => !!host.textContent?.includes('Cannot save settings'));
    assert(
      !!host.querySelector('dialog[open]') && savedSettings === null,
      'Failed settings save keeps the draft open without accepting it',
    );
    failSettings = false;
    button('Apply settings').click();
    await until(() => !host.querySelector('dialog[open]'));
    await until(() => !menuState?.busy);
    menuAction('saveProject');
    await until(
      () =>
        saves.length === 1 &&
        !(!menuState || menuState.busy || !menuState.available),
    );
    assert(
      (saves[0]!.options as { timing: { cueGapMs: number } }).timing
        .cueGapMs === 200,
      'Failed import preserves policy and project save retains settings',
    );
    document.querySelector('#result')!.textContent =
      'PASS: all 14 magnetic timing checks';
  } catch (e) {
    document.querySelector('#result')!.textContent = 'FAIL: ' + String(e);
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
