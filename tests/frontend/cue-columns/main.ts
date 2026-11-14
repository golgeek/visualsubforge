import { visualCheckpoint } from '../visual-checkpoint';
import { emptyQuality } from '../empty-quality';
import { mount, tick, unmount } from 'svelte';
import { qualityView, styledDocument } from '../ipc-contract-samples';
import App from '../../../src/App.svelte';
import {
  defaultPreferences,
  emptyDocument,
  emptyStatus,
  type MediaAction,
  type Preferences,
  type DocumentSnapshot,
} from '../../../src/lib/client';

// Production workspace, modeled IPC. Visibility follows accepted media status;
// no requests or document ownership are replaced by UI-only action flags.
const host = document.querySelector<HTMLElement>('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
const media = { ...emptyStatus, available: true, surfaceAboveWebview: true };
const snapshot: DocumentSnapshot = {
  ...emptyDocument,
  loaded: true,
  generation: 1,
  cues: Array.from({ length: 10000 }, (_, index) => ({
    id: index + 1,
    startMs: index * 2000,
    endMs: index * 2000 + 1500,
    text: `Subtitle ${index + 1}`,
  })),
};
const callbacks = new Map<number, (value: unknown) => void>();
let listener = 0;
let occluded = false;
let stored = structuredClone(defaultPreferences.layout);
let saves = 0;
let nativeBounds: MediaAction | null = null;
let holdHide: (() => void) | null = null;
let delayHide = false;
let boundsBeforeRestore = false;
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
    (b) =>
      b.getAttribute('aria-label') === label || b.textContent?.trim() === label,
  );
}
async function until(predicate: () => boolean) {
  const end = performance.now() + 5000;
  while (!predicate()) {
    assert(performance.now() < end, 'Timed out: ' + host.textContent);
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  await tick();
  await visualCheckpoint();
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
        action?: MediaAction;
        handler?: number;
        event?: string;
        layout?: Preferences['layout'];
      },
    ) {
      switch (command) {
        case 'recent_list':
        case 'recent_capture':
        case 'recent_forget':
          return [];
        case 'quality_read':
          return {
            ...emptyQuality(args),
            findingsCount: 2,
            filteredCount: 2,
            findings: [
              { ...qualityView.findings[0]!, cueId: '2', relatedCueId: '3' },
              {
                ...qualityView.findings[0]!,
                cueId: '9000',
                relatedCueId: null,
              },
            ],
          };
        case 'quality_cancel':
          return null;
        case 'plugin:event|listen':
          if (args.event !== 'workspace-action') return 2;
          listener = args.handler!;
          return 1;
        case 'plugin:event|unlisten':
        case 'update_menu':
          return;
        case 'preferences_get':
          return {
            ...structuredClone(defaultPreferences),
            layout: structuredClone(stored),
          };
        case 'layout_save':
          stored = structuredClone(args.layout!);
          ++saves;
          return null;
        case 'document_action':
          return structuredClone(snapshot);
        case 'media_action':
          if (args.action?.kind === 'occlude') {
            if (args.action.occluded && delayHide)
              await new Promise<void>((resolve) => {
                holdHide = resolve;
              });
            if (!args.action.occluded) {
              const rect = host
                .querySelector('.video-space')
                ?.getBoundingClientRect();
              if (rect && nativeBounds?.kind === 'layout')
                boundsBeforeRestore =
                  Math.abs(rect.x - nativeBounds.x) < 1 &&
                  Math.abs(rect.width - nativeBounds.width) < 1;
            }
            occluded = args.action.occluded;
          }
          if (args.action?.kind === 'layout') nativeBounds = args.action;
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

async function choose(label: string) {
  await until(() => !!button('Layout') && !button('Layout')!.disabled);
  button('Layout')!.click();
  await until(
    () => !!host.querySelector('dialog[aria-label="Workspace layout"][open]'),
  );
  button(label)!.click();
  await until(
    () =>
      !host.querySelector('dialog[open]') &&
      !button('Layout')!.disabled &&
      !occluded,
  );
}

async function openColumns() {
  button('Columns ▾')!.click();
  await until(
    () => !!host.querySelector('dialog[aria-label="Cue columns"][open]'),
  );
}
function checkbox(id: string) {
  return host.querySelector<HTMLInputElement>(
    `dialog [data-column="${id}"] input`,
  )!;
}
function columns() {
  return [...host.querySelectorAll<HTMLElement>('.column-heading')].map(
    (c) => c.dataset.column,
  );
}
function setQuery(text: string) {
  const input = host.querySelector<HTMLInputElement>(
    '[aria-label="Filter cues by text or number"]',
  )!;
  input.value = text;
  input.dispatchEvent(new Event('input', { bubbles: true }));
}
async function run() {
  let app = mount(App, { target: host });
  try {
    await until(
      () =>
        !!listener && !!button('Columns ▾') && !button('Columns ▾')!.disabled,
    );
    await until(() => !!button('Issues 3'));
    delayHide = true;
    button('Columns ▾')!.click();
    await until(() => !!holdHide);
    assert(
      !host.querySelector('dialog[open]'),
      'Chooser preceded native hide acknowledgement',
    );
    delayHide = false;
    holdHide!();
    holdHide = null;
    await until(() => !!host.querySelector('dialog[open]'));
    assert(occluded, 'Chooser did not hide native video');
    assert(
      checkbox('issue').disabled && checkbox('text').disabled,
      'Mandatory columns can be hidden',
    );
    assert(
      !checkbox('actor') && !checkbox('style'),
      'ASS-only choices shown for plain subtitles',
    );
    checkbox('start').click();
    checkbox('duration').click();
    const source = host.querySelector('[data-column="duration"][draggable]')!;
    const destination = host.querySelector('dialog [data-column="number"]')!;
    source.dispatchEvent(new Event('dragstart', { bubbles: true }));
    destination.dispatchEvent(
      new Event('drop', { bubbles: true, cancelable: true }),
    );
    source.dispatchEvent(new Event('dragend', { bubbles: true }));
    button('Done')!.click();
    await until(() => !host.querySelector('dialog[open]') && !occluded);
    assert(
      columns()[1] === 'duration' && !columns().includes('start'),
      'Visibility/order did not reach table',
    );
    passed(
      'Chooser waits for native hiding, pins required columns, and applies visibility and drag order',
    );

    const edge = host.querySelector<HTMLElement>(
      '[aria-label="Resize Duration column"]',
    )!;
    edge.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }),
    );
    await until(
      () =>
        stored.timing.columns.items.find((c) => c.id === 'duration')?.width ===
        88,
    );
    edge.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    await until(
      () =>
        stored.timing.columns.items.find((c) => c.id === 'duration')?.width !==
        88,
    );
    await choose('Translation');
    assert(boundsBeforeRestore, 'Preset restored before current native bounds');
    assert(
      columns().includes('start') && !columns().includes('duration'),
      'Preset inherited another preset columns',
    );
    await choose('Timing');
    await until(() => stored.active === 'timing');
    await unmount(app);
    app = mount(App, { target: host });
    await until(() => !!button('Columns ▾') && !button('Columns ▾')!.disabled);
    assert(
      columns()[1] === 'duration' && !columns().includes('start'),
      'Saved columns did not survive remount',
    );
    assert(saves > 0, 'No persistence happened');
    callbacks.get(listener)!({ payload: 'resetLayout' });
    await until(() => !button('Layout')!.disabled && !occluded);
    assert(
      columns()[1] === 'duration',
      'Reset Pane Sizes reset column choices',
    );
    passed(
      'Resize and visible-row fit persist; preset choices remain independent across reload',
    );

    setQuery('#9000');
    await until(() => host.querySelectorAll('.cue-row').length === 1);
    assert(
      host.querySelector('[data-cell="number"]')?.textContent === '9000',
      'Filter lost original index',
    );
    setQuery('');
    await tick();
    await until(() => !!button('Issues 3'));
    button('Issues 3')!.click();
    await until(() => host.querySelectorAll('.cue-row').length === 3);
    assert(
      [...host.querySelectorAll('[data-cell="number"]')]
        .map((c) => c.textContent)
        .join(',') === '2,3,9000',
      'Issue filter lost related or distant cues',
    );
    button('Issues 3')!.click();
    await tick();
    host.querySelector<HTMLButtonElement>('.cue-row')!.click();
    await until(() => !button('Columns ▾')!.disabled);
    assert(!button('Jump to playhead'), 'Row click paused following');
    const list = host.querySelector<HTMLElement>('.cue-list')!;
    list.scrollTop = 1000;
    list.dispatchEvent(new Event('scroll'));
    await until(() => !!button('Jump to playhead'));
    button('Jump to playhead')!.click();
    await until(() => !button('Jump to playhead'));
    passed(
      'Text/# and Issues filters preserve file indices; row clicks keep following and manual scroll pauses until Jump',
    );

    await openColumns();
    host.querySelector<HTMLInputElement>('input[value="comfortable"]')!.click();
    button('Done')!.click();
    await until(
      () =>
        host.querySelector('.cue-row')!.getBoundingClientRect().height === 32,
    );
    await openColumns();
    host.querySelector<HTMLInputElement>('input[value="compact"]')!.click();
    button('Done')!.click();
    await until(
      () =>
        host.querySelector('.cue-row')!.getBoundingClientRect().height === 24,
    );
    const frames: number[] = [];
    let previous = await new Promise<number>(requestAnimationFrame);
    let mounted = 0;
    for (let frame = 0; frame < 120; frame++) {
      list.scrollTop = frame * 1900;
      const time = await new Promise<number>(requestAnimationFrame);
      frames.push(time - previous);
      previous = time;
      mounted = Math.max(mounted, host.querySelectorAll('.cue-row').length);
    }
    assert(mounted < 60, '10k scroll mounted an unbounded list');
    const sorted = [...frames].sort((a, b) => a - b);
    passed(
      `10,000 cues: fixed 24/32 px rows; 120 scroll frames mean ${(frames.reduce((a, b) => a + b, 0) / frames.length).toFixed(1)} ms, p95 ${sorted[114]!.toFixed(1)} ms, max ${sorted[119]!.toFixed(1)} ms; at most ${mounted} mounted rows (${navigator.userAgent})`,
    );
    await unmount(app);
    snapshot.format = structuredClone(styledDocument.format);
    snapshot.cues[0] = {
      ...snapshot.cues[0]!,
      style: 'Default',
      actor: 'Speaker',
    };
    app = mount(App, { target: host });
    await until(() => !!button('Columns ▾') && !button('Columns ▾')!.disabled);
    await openColumns();
    assert(
      checkbox('style') && checkbox('actor'),
      'ASS columns unavailable for ASS',
    );
    checkbox('style').click();
    checkbox('actor').click();
    button('Done')!.click();
    await until(() => !!host.querySelector('[data-cell="actor"]'));
    assert(
      host.querySelector('[data-cell="actor"]')?.textContent === 'Speaker',
      'Actor projection missing',
    );
    passed('ASS-only Style and Actor choices render accepted metadata');
    heading.textContent = `PASS: all ${output.children.length} cue column checks`;
  } finally {
    await unmount(app);
  }
}
run().catch((error) => {
  heading.textContent = 'FAIL: ' + String(error);
  console.error(error);
});
