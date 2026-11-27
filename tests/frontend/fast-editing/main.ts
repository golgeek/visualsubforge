import { emptyQuality } from '../empty-quality';
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
  surfaceAboveWebview: true,
  loaded: true,
  session: 1,
  durationSeconds: 100,
  positionSeconds: 5,
  paused: false,
};
let occluded = false;
let holdDelete = false;
let releaseDelete: (() => void) | undefined;
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
  const found = [
    ...host.querySelectorAll<HTMLButtonElement>(
      'button:not(dialog:not([open]) button)',
    ),
  ].find(
    (b) =>
      b.getAttribute('aria-label') === label || b.textContent?.trim() === label,
  );
  if (!found) throw new Error(`Missing ${label}`);
  return found;
}
function transportGeometry() {
  return JSON.stringify(
    [
      ...host.querySelectorAll(
        '.transport button, .transport output, .transport input, .transport select',
      ),
    ].map((el) => {
      const { x, y, width, height } = el.getBoundingClientRect();
      return [x, y, width, height];
    }),
  );
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
        return structuredClone({
          ...defaultPreferences,
          zoomWaveformToCue: false,
        });
      switch (command) {
        case 'recent_list':
        case 'recent_capture':
        case 'recent_forget':
          return [];
        case 'quality_read':
          return emptyQuality(args);
        case 'quality_cancel':
          return null;
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
          if (args.action?.kind === 'occlude') occluded = args.action.occluded;
          if (args.action?.kind === 'loop') {
            media.loopActive = true;
            loops.push([args.action.start, args.action.end]);
          }
          if (args.action?.kind === 'clearLoop') media.loopActive = false;
          if (args.action?.kind !== 'status' && args.action?.kind !== 'layout')
            transport.push(args.action!.kind);
          if (args.action?.kind === 'pause') media.paused = args.action.paused;
          if (args.action?.kind === 'playSelection') {
            media.paused = false;
            media.positionSeconds = args.action.start;
          }
          if (args.action?.kind === 'seekToCue')
            media.positionSeconds = args.action.start;
          if (
            args.action?.kind === 'clearPlaySelection' &&
            args.action.seek != null
          )
            media.positionSeconds = args.action.seek;
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
            if (holdDelete) {
              await new Promise<void>((resolve) => {
                releaseDelete = resolve;
              });
              throw new Error('Test rejected deletion');
            }
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
    await until(
      () => !(!menuState || !menuState!.enabled.includes('settings')),
    );
    const canvas = host.querySelector<HTMLCanvasElement>(
      'canvas[aria-label^="Audio waveform:"]',
    )!;
    const editor = host.querySelector('textarea')!;
    const mode = host.querySelector<HTMLInputElement>(
      '[aria-label="Tap-to-time · hold Space"]',
    )!;
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
    const playingGeometry = transportGeometry();
    assert(button('Pause').textContent?.trim() === '', 'Pause must be an icon');
    key(editor, ' ', { altKey: true });
    await until(
      () =>
        media.paused &&
        !(!menuState || !menuState!.enabled.includes('settings')),
    );
    assert(
      transportGeometry() === playingGeometry,
      'Play/Pause moved a transport control',
    );
    assert(document.activeElement === editor, 'Transport stole text focus');
    key(editor, ' ', { altKey: true });
    await until(
      () =>
        !media.paused &&
        !(!menuState || !menuState!.enabled.includes('settings')),
    );
    passed('Modified Space controls playback while text remains focused');
    key(editor, 'Escape');
    await until(() => document.activeElement === canvas);
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
    // Structural commands retain selected IDs. Start this single-target trial
    // with only the newly created cue selected, then right-click outside it.
    host.querySelector<HTMLButtonElement>('[data-cue-id="4"]')!.click();
    await until(
      () =>
        host.querySelectorAll('.cue-row[aria-pressed="true"]').length === 1 &&
        !!menuState?.enabled.includes('settings'),
    );
    const before = transport.filter(
      (k) => k === 'seek' || k === 'seekToCue',
    ).length;
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
    [...host.querySelectorAll<HTMLButtonElement>('[role=menuitem]')]
      .find((b) => b.textContent?.trim() === 'Delete cue')!
      .click();
    await until(
      () =>
        snapshot.cues.length === 3 &&
        !(!menuState || !menuState!.enabled.includes('settings')),
    );
    assert(
      !snapshot.cues.some((c) => c.id === 1) &&
        transport.filter((k) => k === 'seek' || k === 'seekToCue').length ===
          before,
      'Wrong cue deleted / video sought',
    );
    passed('Row context deletion targets the clicked cue without seeking');
    canvas.focus();
    // Adding result rows above the app changes its position. In Linux WebKit,
    // reusing the earlier rectangle hits the selection band instead of the cue.
    const cueBounds = canvas.getBoundingClientRect();
    canvas.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: cueBounds.left + (cueBounds.width * 5.5) / 20,
        clientY: cueBounds.top + 35,
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

    const anchor = host.querySelector<HTMLElement>(
      '[data-tooltip-action="splitCue"]',
    )!;
    const video = host.querySelector('.video-space')!.getBoundingClientRect();
    // At opposite edges only the anchor or only the rendered label crosses.
    const anchorHeight = anchor.getBoundingClientRect().height;
    for (const top of [video.top - anchorHeight - 2, video.bottom - 2]) {
      anchor.style.position = 'fixed';
      anchor.style.left = video.left + 12 + 'px';
      anchor.style.top = top + 'px';
      anchor.dispatchEvent(new PointerEvent('pointerenter'));
      await until(() => !!host.querySelector('[role=tooltip]') && occluded);
      anchor.dispatchEvent(new PointerEvent('pointerleave'));
      await until(() => !host.querySelector('[role=tooltip]') && !occluded);
    }
    anchor.style.removeProperty('position');
    anchor.style.removeProperty('left');
    anchor.style.removeProperty('top');
    const menuRow = host.querySelector<HTMLButtonElement>('[data-cue-id="4"]')!;
    const openMenu = (overlapping: boolean) => {
      menuRow.dispatchEvent(
        new MouseEvent('contextmenu', {
          bubbles: true,
          cancelable: true,
          clientX: overlapping ? video.left + 8 : 12,
          clientY: overlapping ? video.top + 8 : innerHeight - 12,
        }),
      );
    };
    // Close before the hover delay expires: admission must have cancelled it.
    anchor.dispatchEvent(new PointerEvent('pointerenter'));
    openMenu(false);
    await until(() => !!host.querySelector('.cue-menu'));
    key(document.activeElement!, 'Escape');
    await until(() => !host.querySelector('.cue-menu'));
    await new Promise((resolve) => setTimeout(resolve, 400));
    assert(
      !host.querySelector('[role=tooltip]'),
      'A retired hover must not reappear',
    );
    for (const overlapping of [true, false]) {
      if (!overlapping) {
        anchor.dispatchEvent(new PointerEvent('pointerenter'));
        await until(() => !!host.querySelector('[role=tooltip]'));
      }
      openMenu(overlapping);
      await until(
        () => !!host.querySelector('.cue-menu') && occluded === overlapping,
      );
      assert(
        !host.querySelector('[role=tooltip]'),
        'Opening a cue menu must retire the visible tooltip even away from video',
      );
      anchor.dispatchEvent(new PointerEvent('pointerenter'));
      await new Promise((resolve) => setTimeout(resolve, 400));
      assert(
        !host.querySelector('[role=tooltip]'),
        'An open cue menu must block hover even away from video',
      );
      anchor.dispatchEvent(new PointerEvent('pointerleave'));
      await settle();
      assert(
        occluded === overlapping,
        'Tooltip dismissal preserves the menu overlap',
      );
      holdDelete = true;
      host
        .querySelector<HTMLButtonElement>(
          '.cue-menu [aria-label="Delete cue"]',
        )!
        .click();
      await until(() => !!releaseDelete);
      assert(
        !host.querySelector('.cue-menu') && occluded === overlapping,
        'Pending commit retains its menu geometry',
      );
      releaseDelete!();
      releaseDelete = undefined;
      holdDelete = false;
      await until(() => !occluded && !!menuState?.enabled.includes('settings'));
    }
    canvas.focus();

    const count = snapshot.cues.length;
    key(canvas, ' ');
    key(canvas, ' ', {}, 'keyup');
    await until(
      () =>
        media.paused &&
        !(!menuState || !menuState!.enabled.includes('settings')),
    );
    assert(
      snapshot.cues.length === count && document.activeElement === canvas,
      'Tap created cue or lost timing focus',
    );
    passed(
      'Short Space pauses between source-clock samples without creating a cue',
    );
    key(canvas, ' ');
    await until(
      () =>
        !media.paused &&
        !(!menuState || !menuState!.enabled.includes('settings')),
    );
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
          !(!menuState || !menuState!.enabled.includes('settings')),
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
    await until(() => document.activeElement === canvas);
    assert(
      document.activeElement === canvas,
      'Escape did not return to timing',
    );
    passed(
      'Enter explicitly edits text with ordinary spaces; Escape returns to continuous timing',
    );
    const pan = host.querySelector<HTMLCanvasElement>(
      '[aria-label="Timeline minimap"]',
    )!;
    const follow = [...host.querySelectorAll('label')]
      .find((l) => l.textContent?.trim() === 'Follow playhead')!
      .querySelector('input')!;
    media.positionSeconds = 18;
    await until(() => Number(pan.getAttribute('aria-valuenow')) === 10);
    key(canvas, ' ');
    media.positionSeconds = 19;
    await until(() => Number(pan.getAttribute('aria-valuenow')) === 11);
    media.positionSeconds = 21;
    await until(() => Number(pan.getAttribute('aria-valuenow')) === 13);
    await new Promise((resolve) => setTimeout(resolve, 450));
    key(canvas, ' ', {}, 'keyup');
    await until(
      () =>
        snapshot.cues.some((c) => c.startMs === 18000 && c.endMs === 21000) &&
        !(!menuState || !menuState!.enabled.includes('settings')),
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
          clientX:
            rect.left +
            (rect.width * (19 - Number(pan.getAttribute('aria-valuenow')))) /
              20,
          clientY: rect.top + 40,
        }),
      );
    const seekCount = transport.filter(
      (k) => k === 'seek' || k === 'seekToCue',
    ).length;
    cueClick('pointerdown');
    cueClick('pointerup');
    await until(
      () => !(!menuState || !menuState!.enabled.includes('settings')),
    );
    assert(
      host
        .querySelector('.cue-row.selected')
        ?.textContent?.includes('00:00:18,000') === true &&
        transport.filter((k) => k === 'seek' || k === 'seekToCue').length ===
          seekCount + 1 &&
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
    const loop = button('Loop range');
    const beforeLoop = transportGeometry();
    assert(
      loop.getAttribute('aria-pressed') === 'false',
      'Loop must start unpressed',
    );
    loop.click();
    await until(
      () =>
        loops.length > 0 &&
        !(!menuState || !menuState!.enabled.includes('settings')),
    );
    const edge = host.querySelector<HTMLButtonElement>(
      '[aria-label^="Cue end boundary"]',
    )!;
    assert(
      loop.getAttribute('aria-pressed') === 'true' &&
        beforeLoop === transportGeometry(),
      'Starting the loop moved controls or failed to show its state',
    );
    const editRevision = snapshot.revision;
    key(edge, 'ArrowRight', { shiftKey: true });
    await until(() => loops.at(-1)?.[1] === 21.1);
    assert(
      snapshot.revision === editRevision,
      'Preview committed before release',
    );
    key(edge, 'Escape');
    await until(
      () =>
        loops.at(-1)?.[1] === 21 &&
        !(!menuState || !menuState!.enabled.includes('settings')),
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
        !(!menuState || !menuState!.enabled.includes('settings')),
    );
    assert(
      host.textContent?.includes('Range 18.000–21.100 s') === true,
      'Committed range did not follow timing',
    );
    passed(
      'Boundary release commits one edit and keeps the live loop at accepted bounds',
    );
    const beforeClear = transportGeometry();
    loop.click();
    await until(
      () => !(!menuState || !menuState!.enabled.includes('settings')),
    );
    assert(
      loop.getAttribute('aria-pressed') === 'false' &&
        beforeClear === transportGeometry(),
      'Clearing the loop moved controls or failed to show its state',
    );
    const loopsBefore = loops.length;
    key(edge, 'ArrowRight');
    key(edge, 'ArrowRight', {}, 'keyup');
    await until(
      () =>
        snapshot.revision === editRevision + 2 &&
        !(!menuState || !menuState!.enabled.includes('settings')),
    );
    assert(
      loops.length === loopsBefore,
      'Boundary edit reenabled cleared loop',
    );
    passed(
      'Clear loop prevents subsequent timing previews from reenabling playback loops',
    );
    key(canvas, ' ', { altKey: true });
    await until(
      () =>
        media.paused &&
        !(!menuState || !menuState!.enabled.includes('settings')),
    );
    const firstRow = host.querySelector<HTMLButtonElement>('.cue-row')!;
    firstRow.click();
    await until(
      () =>
        transport.filter((k) => k === 'seek' || k === 'seekToCue').length ===
          seekCount + 4 &&
        !(!menuState || !menuState!.enabled.includes('settings')),
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
    const detachedStart = Number(pan.getAttribute('aria-valuenow'));
    media.positionSeconds = 40;
    await settle();
    assert(
      Number(pan.getAttribute('aria-valuenow')) === detachedStart,
      'Disabled Follow moved viewport',
    );
    follow.click();
    await until(() => Number(pan.getAttribute('aria-valuenow')) === 32);
    passed(
      'Follow can be disabled and resumed independently after held timing',
    );
    loop.click();
    await until(
      () => loop.getAttribute('aria-pressed') === 'true' && !loop.disabled,
    );
    const sentLoops = loops.length;
    // Drag outwards from the left viewport edge. Both ends clamp to its start.
    for (const [type, x] of [
      ['pointerdown', rect.left],
      ['pointerup', rect.left - 10],
    ] as const) {
      canvas.dispatchEvent(
        new PointerEvent(type, {
          pointerId: 1,
          isPrimary: true,
          button: 0,
          bubbles: true,
          clientX: x,
          clientY: rect.top + 10,
          altKey: true,
        }),
      );
      await tick();
    }
    assert(
      loops.length === sentLoops,
      'The collapsed range was sent to the player',
    );
    assert(
      !loop.disabled && loop.getAttribute('aria-pressed') === 'true',
      'An active loop must remain clearable',
    );
    loop.click();
    await until(() => loop.getAttribute('aria-pressed') === 'false');
    assert(loop.disabled, 'A collapsed selection still enables Loop range');
    passed('A zero-length selection cannot start a playback loop');
    const selectPlays = () =>
      transport.filter((kind) => kind === 'playSelection').length;
    async function toggle(paused: boolean) {
      await until(() => !button(paused ? 'Pause' : 'Play').disabled);
      button(paused ? 'Pause' : 'Play').click();
      await until(
        () =>
          media.paused === paused && !!menuState?.enabled.includes('settings'),
      );
    }
    if (!media.paused) await toggle(true);
    const initialPlays = selectPlays();
    await toggle(false);
    assert(selectPlays() === initialPlays, 'Ordinary Play must stay unbounded');
    await toggle(true);
    firstRow.click();
    await until(
      () => !loop.disabled && !!menuState?.enabled.includes('settings'),
    );
    await toggle(false);
    assert(
      selectPlays() === initialPlays + 1 &&
        media.positionSeconds === snapshot.cues[0]!.startMs / 1000,
      'Play must send the selected cue bounds',
    );
    loop.click();
    await until(
      () => media.loopActive && !!menuState?.enabled.includes('settings'),
    );
    await toggle(true);
    await toggle(false);
    assert(
      selectPlays() === initialPlays + 1 && media.loopActive,
      'Play must preserve an active loop',
    );
    passed(
      'Selection Play, ordinary Play and active-loop Play use distinct transport policies',
    );
    heading.textContent = 'PASS: all 25 fast-editing checks';
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
