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
const media = {
  ...emptyStatus,
  available: true,
  loaded: true,
  session: 1,
  durationSeconds: 100,
  positionSeconds: 5,
  paused: false,
};
const transport: string[] = [];
const loops: [number, number][] = [];
function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}
function passed(message: string) {
  const item = document.createElement('li');
  item.textContent = `PASS: ${message}`;
  output.append(item);
}
let menuState: Parameters<typeof updateMenu>[0] | undefined;
function button(label: string) {
  const found = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!found) throw new Error(`Missing ${label}`);
  return found;
}
async function until(condition: () => boolean) {
  await tick();
  const deadline = performance.now() + 5000;
  while (!condition()) {
    if (performance.now() > deadline)
      throw new Error(
        'UI response timeout: ' +
          host.querySelector('[role=status]')?.textContent +
          ' cues=' +
          snapshot.cues.length +
          ' menu=' +
          !!host.querySelector('[role=menu]'),
      );
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
    transformCallback: () => 1,
    unregisterCallback: () => {},
    async invoke(
      command: string,
      args: {
        action?: DocumentAction | MediaAction;
        kind?: string;
        state?: Parameters<typeof updateMenu>[0];
      },
    ) {
      if (command === 'preferences_get')
        return structuredClone(defaultPreferences);
      switch (command) {
        case 'plugin:event|listen':
          return 1;
        case 'plugin:event|unlisten':
          return;
        case 'update_menu':
          menuState = args.state;
          return;
        case 'recovery_status':
          return { entries: [], error: null };
        case 'media_action':
          if (args.action?.kind === 'loop')
            loops.push([args.action.start, args.action.end]);
          if (args.action?.kind !== 'status' && args.action?.kind !== 'layout')
            transport.push(args.action!.kind);
          if (args.action?.kind === 'pause') media.paused = args.action.paused;
          if (args.action?.kind === 'seek')
            media.positionSeconds = args.action.seconds;
          return structuredClone(media);
        case 'document_action': {
          const action = args.action as DocumentAction;
          if (action.kind === 'create') {
            const id = Math.max(0, ...snapshot.cues.map((c) => c.id)) + 1;
            const at =
              snapshot.cues.findIndex((c) => c.id === action.afterId) + 1;
            snapshot.cues.splice(at, 0, {
              id,
              startMs: action.startMs,
              endMs: action.endMs,
              text: action.text,
            });
            snapshot = {
              ...snapshot,
              loaded: true,
              generation: 1,
              revision: snapshot.revision + 1,
              dirty: true,
              canUndo: true,
            };
          } else if (action.kind === 'edit') {
            snapshot.cues = snapshot.cues.map((c) =>
              c.id === action.cueId
                ? {
                    ...c,
                    startMs: action.startMs,
                    endMs: action.endMs,
                    text: action.text,
                  }
                : c,
            );
            snapshot.revision++;
          } else if (action.kind === 'delete') {
            snapshot.cues = snapshot.cues.filter((c) => c.id !== action.cueId);
            snapshot.revision++;
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

const settle = () => new Promise((resolve) => setTimeout(resolve, 180));
function key(
  target: EventTarget,
  name: string,
  options: KeyboardEventInit = {},
  type = 'keydown',
) {
  target.dispatchEvent(
    new KeyboardEvent(type, {
      key: name,
      code: name === ' ' ? 'Space' : name,
      bubbles: true,
      cancelable: true,
      ...options,
    }),
  );
}
async function run() {
  const app = mount(App, { target: host });
  try {
    await until(() => !(!menuState || menuState.busy));
    const canvas = host.querySelector('canvas')!;
    const editor = host.querySelector('textarea')!;
    const mode = [...host.querySelectorAll('label')]
      .find((l) => l.textContent?.trim() === 'Timing mode · hold Space')!
      .querySelector('input')!;
    button('Create cue').click();
    await until(() => document.activeElement === editor && !editor.readOnly);
    assert(
      !media.paused && !transport.includes('seek'),
      'Creation moved playback',
    );
    passed(
      'Create selects new cue and focuses text without interrupting playback',
    );
    editor.value = 'First draft';
    editor.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    button('Create cue').click();
    await until(() => snapshot.cues.length === 2 && !editor.readOnly);
    assert(
      snapshot.cues[0]?.text === 'First draft' &&
        document.activeElement === editor,
      'Draft lost/focus lost',
    );
    passed(
      'Create applies previous text draft before creating and focusing the next cue',
    );
    key(editor, ' ', { altKey: true });
    await until(() => media.paused && !(!menuState || menuState.busy));
    assert(document.activeElement === editor, 'Transport stole text focus');
    key(editor, ' ', { altKey: true });
    await until(() => !media.paused && !(!menuState || menuState.busy));
    passed('Modified Space controls playback while text remains focused');
    key(editor, 'Escape');
    await tick();
    assert(document.activeElement === canvas, 'Escape did not focus waveform');
    key(canvas, 'Enter');
    await until(() => document.activeElement === editor);
    assert(document.activeElement === editor, 'Enter did not focus text');
    passed(
      'Escape / Enter switch between waveform and text without losing drafts',
    );
    mode.click();
    await tick();
    assert(
      document.activeElement === canvas,
      'Timing mode did not focus waveform',
    );
    key(canvas, ' ');
    key(canvas, ' ', { repeat: true });
    media.positionSeconds = 6.25;
    await new Promise((resolve) => setTimeout(resolve, 450));
    key(canvas, ' ', {}, 'keyup');
    await until(() => snapshot.cues.length === 3 && !editor.readOnly);
    assert(
      snapshot.cues[2]?.startMs === 5000 &&
        snapshot.cues[2]?.endMs === 6250 &&
        document.activeElement === canvas &&
        !media.paused,
      'Wrong marked interval/focus/playback',
    );
    passed(
      'Held Space creates one source-clock interval on release, ignores repeat, and retains waveform focus',
    );
    key(editor, 'Escape');
    key(canvas, ' ');
    media.positionSeconds = 7;
    await settle();
    key(canvas, 'Escape');
    key(canvas, ' ', {}, 'keyup');
    await settle();
    assert(snapshot.cues.length === 3, 'Cancelled hold created cue');
    passed('Escape cancels a held timing gesture');
    key(canvas, ' ');
    window.dispatchEvent(new Event('blur'));
    media.positionSeconds = 8;
    await settle();
    key(canvas, ' ', {}, 'keyup');
    await settle();
    assert(snapshot.cues.length === 3, 'Blur created cue');
    passed('Window blur cancels held timing');
    // The marked interval is the accepted waveform selection, overlapping cues.
    const bounds = canvas.getBoundingClientRect();
    canvas.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: bounds.left + (bounds.width * 5.5) / 20,
        clientY: bounds.top + 35,
      }),
    );
    await until(() => !!host.querySelector('[role=menu]'));
    assert(
      !!host.textContent?.includes('Create cue from selection'),
      'Selection menu missing creation',
    );
    button('Create cue from selection').click();
    await until(() => snapshot.cues.length === 4 && !editor.readOnly);
    assert(
      snapshot.cues[3]?.startMs === 5000 && snapshot.cues[3]?.endMs === 6250,
      'Selection range changed',
    );
    passed('Waveform selection context menu creates its exact interval');
    const row = host.querySelector<HTMLButtonElement>('.cue-row')!;
    row.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: 80,
        clientY: 200,
      }),
    );
    await until(() => !!host.querySelector('[role=menu]'));
    const before = transport.filter((k) => k === 'seek').length;
    [...host.querySelectorAll<HTMLButtonElement>('[role=menuitem]')]
      .find((b) => b.textContent?.trim() === 'Delete cue')!
      .click();
    await until(
      () => snapshot.cues.length === 3 && !(!menuState || menuState.busy),
    );
    assert(
      !snapshot.cues.some((c) => c.id === 1) &&
        transport.filter((k) => k === 'seek').length === before,
      'Wrong cue deleted / video sought',
    );
    passed('Row context deletion targets the clicked cue without seeking');
    canvas.focus();
    canvas.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: bounds.left + (bounds.width * 5.5) / 20,
        clientY: bounds.top + 35,
      }),
    );
    await until(() => !!host.querySelector('[role=menu]'));
    assert(!!button('Edit cue text'), 'Existing cue menu missing');
    key(document.activeElement!, 'ArrowDown');
    key(document.activeElement!, 'Escape');
    await until(() => !host.querySelector('[role=menu]'));
    assert(
      snapshot.cues.length === 3 && document.activeElement === canvas,
      'Menu cancellation mutated / failed focus',
    );
    passed(
      'Existing waveform cues expose actions; menu keyboard cancellation restores focus',
    );

    const count = snapshot.cues.length;
    key(canvas, ' ');
    key(canvas, ' ', {}, 'keyup');
    await until(() => media.paused && !(!menuState || menuState.busy));
    assert(
      snapshot.cues.length === count && document.activeElement === canvas,
      'Tap created cue or lost timing focus',
    );
    passed(
      'Short Space pauses between source-clock samples without creating a cue',
    );
    key(canvas, ' ');
    await until(() => !media.paused && !(!menuState || menuState.busy));
    key(canvas, ' ', { repeat: true });
    key(canvas, ' ', {}, 'keyup');
    await settle();
    assert(
      snapshot.cues.length === count && !media.paused,
      'Playback-start release created a cue or paused',
    );
    passed(
      'Space starts paused playback; its repeats and release cannot mark a cue',
    );
    for (const end of [9.25, 10.5]) {
      key(document.activeElement!, ' ');
      media.positionSeconds = end;
      await new Promise((resolve) => setTimeout(resolve, 450));
      key(document.activeElement!, ' ', {}, 'keyup');
      await until(
        () =>
          snapshot.cues.some((c) => c.endMs === end * 1000) &&
          !(!menuState || menuState.busy),
      );
      assert(
        document.activeElement === canvas && mode.checked && !media.paused,
        'Continuous timing lost mode/focus/playback',
      );
    }
    assert(
      snapshot.cues.length === count + 2,
      'Repeated holds did not create two cues',
    );
    passed(
      'Consecutive Space holds create separate cues without refocusing or leaving timing mode',
    );
    key(canvas, 'Enter');
    await until(() => document.activeElement === editor);
    const space = new KeyboardEvent('keydown', {
      key: ' ',
      code: 'Space',
      bubbles: true,
      cancelable: true,
    });
    editor.dispatchEvent(space);
    key(editor, ' ', {}, 'keyup');
    await settle();
    assert(
      !space.defaultPrevented &&
        snapshot.cues.length === count + 2 &&
        !media.paused,
      'Explicit text editing intercepted a space',
    );
    key(editor, 'Escape');
    await tick();
    assert(
      document.activeElement === canvas,
      'Escape did not return to timing',
    );
    passed(
      'Enter explicitly edits text with ordinary spaces; Escape returns to continuous timing',
    );
    const pan = host.querySelector<HTMLInputElement>('.pan input')!;
    const follow = [...host.querySelectorAll('label')]
      .find((l) => l.textContent?.trim() === 'Follow playback')!
      .querySelector('input')!;
    media.positionSeconds = 18;
    await until(() => Number(pan.value) === 10);
    key(canvas, ' ');
    media.positionSeconds = 19;
    await until(() => Number(pan.value) === 11);
    media.positionSeconds = 21;
    await until(() => Number(pan.value) === 13);
    await new Promise((resolve) => setTimeout(resolve, 450));
    key(canvas, ' ', {}, 'keyup');
    await until(
      () =>
        snapshot.cues.some((c) => c.startMs === 18000 && c.endMs === 21000) &&
        !(!menuState || menuState.busy),
    );
    assert(
      follow.checked && document.activeElement === canvas && !media.paused,
      'Follow or timing focus lost',
    );
    passed(
      'Follow scrolls before and across the old viewport edge during a held Space cue',
    );
    key(canvas, 'ArrowUp', { altKey: true });
    await until(() => document.activeElement === editor && !editor.readOnly);
    key(editor, 'Escape');
    await tick();
    const rect = canvas.getBoundingClientRect();
    let capture: number | null = null;
    canvas.setPointerCapture = (id) => {
      capture = id;
    };
    canvas.hasPointerCapture = (id) => capture === id;
    canvas.releasePointerCapture = () => {
      capture = null;
    };
    const cueClick = (type: string) =>
      canvas.dispatchEvent(
        new PointerEvent(type, {
          pointerId: 1,
          isPrimary: true,
          button: 0,
          bubbles: true,
          cancelable: true,
          clientX: rect.left + (rect.width * (19 - Number(pan.value))) / 20,
          clientY: rect.top + 40,
        }),
      );
    const seekCount = transport.filter((k) => k === 'seek').length;
    cueClick('pointerdown');
    cueClick('pointerup');
    await until(() => !(!menuState || menuState.busy));
    assert(
      host
        .querySelector('.cue-row.selected')
        ?.textContent?.includes('00:00:18,000') === true &&
        transport.filter((k) => k === 'seek').length === seekCount + 1 &&
        media.positionSeconds === 18 &&
        !media.paused,
      'Waveform click did not select and seek to cue start while playing',
    );
    passed(
      'Waveform cue selection seeks to its start and preserves playing state',
    );
    assert(
      host.textContent?.includes('Range 18.000–21.000 s') === true,
      'Cue selection retained the preceding range',
    );
    passed('Cue click replaces the previous waveform range with cue bounds');
    cueClick('pointerdown');
    cueClick('pointerup');
    await until(() => media.positionSeconds === 19);
    assert(
      host.textContent?.includes('Range 18.000–21.000 s') === true &&
        !media.paused,
      'Repeat click changed cue range or playback state',
    );
    passed(
      'Second click seeks inside selected cue without replacing its range',
    );
    button('Loop range').click();
    await until(() => loops.length > 0 && !(!menuState || menuState.busy));
    const edge = host.querySelector<HTMLButtonElement>(
      '[aria-label^="Cue end boundary"]',
    )!;
    const editRevision = snapshot.revision;
    key(edge, 'ArrowRight', { shiftKey: true });
    await until(() => loops.at(-1)?.[1] === 21.1);
    assert(
      snapshot.revision === editRevision,
      'Preview committed before release',
    );
    key(edge, 'Escape');
    await until(
      () => loops.at(-1)?.[1] === 21 && !(!menuState || menuState.busy),
    );
    assert(
      snapshot.revision === editRevision &&
        host.textContent?.includes('Range 18.000–21.000 s') === true,
      'Cancellation failed to restore range/history',
    );
    passed(
      'Timing preview updates enabled loop before commit; Escape restores range and loop',
    );
    key(edge, 'ArrowRight', { shiftKey: true });
    await until(() => loops.at(-1)?.[1] === 21.1);
    key(edge, 'ArrowRight', {}, 'keyup');
    await until(
      () =>
        snapshot.revision === editRevision + 1 &&
        !(!menuState || menuState.busy),
    );
    assert(
      host.textContent?.includes('Range 18.000–21.100 s') === true,
      'Committed range did not follow timing',
    );
    passed(
      'Boundary release commits one edit and keeps the live loop at accepted bounds',
    );
    button('Clear loop').click();
    await until(() => !(!menuState || menuState.busy));
    const loopsBefore = loops.length;
    key(edge, 'ArrowRight');
    key(edge, 'ArrowRight', {}, 'keyup');
    await until(
      () =>
        snapshot.revision === editRevision + 2 &&
        !(!menuState || menuState.busy),
    );
    assert(
      loops.length === loopsBefore,
      'Boundary edit reenabled cleared loop',
    );
    passed(
      'Clear loop prevents subsequent timing previews from reenabling playback loops',
    );
    key(canvas, ' ', { altKey: true });
    await until(() => media.paused && !(!menuState || menuState.busy));
    const firstRow = host.querySelector<HTMLButtonElement>('.cue-row')!;
    firstRow.click();
    await until(
      () =>
        transport.filter((k) => k === 'seek').length === seekCount + 4 &&
        !(!menuState || menuState.busy),
    );
    assert(
      media.positionSeconds === (snapshot.cues[0]?.startMs ?? -1) / 1000 &&
        media.paused &&
        firstRow.classList.contains('selected'),
      'Row click did not select and seek while paused',
    );
    passed('Cue-list click seeks to cue start and preserves paused state');
    follow.click();
    await tick();
    const detachedStart = Number(pan.value);
    media.positionSeconds = 40;
    await settle();
    assert(
      Number(pan.value) === detachedStart,
      'Disabled Follow moved viewport',
    );
    follow.click();
    await until(() => Number(pan.value) === 32);
    passed(
      'Follow can be disabled and resumed independently after held timing',
    );
    heading.textContent = 'PASS: all 23 fast-editing checks';
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
