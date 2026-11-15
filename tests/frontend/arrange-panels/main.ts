import { visualCheckpoint } from '../visual-checkpoint';
import { pauseChecks } from './pause';
import { ownerChecks } from './owners';
import { emptyQuality } from '../empty-quality';
import { mount, tick, unmount } from 'svelte';
import App from '../../../src/App.svelte';
import {
  defaultPreferences,
  emptyDocument,
  emptyStatus,
  type MediaAction,
  type Preferences,
  type SaveProjectRequest,
  type DocumentAction,
} from '../../../src/lib/client';

// Production workspace, modeled IPC. Visibility follows accepted media status;
// no requests or document ownership are replaced by UI-only action flags.
const host = document.querySelector<HTMLElement>('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
const media = {
  ...emptyStatus,
  available: true,
  loaded: true,
  surfaceAboveWebview: true,
};
const snapshot = {
  ...emptyDocument,
  loaded: true,
  path: '/captions.srt',
  generation: 1,
  cues: Array.from({ length: 80 }, (_, index) => ({
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
const projectSaves: SaveProjectRequest[] = [];
const documentActions: string[] = [];
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
    (b) => b.textContent?.trim() === label,
  );
}
async function until(predicate: () => boolean) {
  await tick();
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
        action?: MediaAction | DocumentAction;
        request?: SaveProjectRequest;
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
          return emptyQuality(args);
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
          documentActions.push(args.action!.kind);
          return structuredClone(snapshot);
        case 'choose_file':
          return '/arrange.vsfproject';
        case 'project_save':
          // Model Tauri JSON serialization, which unwraps reactive option proxies.
          projectSaves.push(
            JSON.parse(JSON.stringify(args.request!)) as SaveProjectRequest,
          );
          return { path: '/arrange.vsfproject', token: projectSaves.length };
        case 'project_detach':
          return null;
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

async function openArrange() {
  await until(
    () =>
      !!listener &&
      !!host.querySelector('main') &&
      host.querySelector('main')?.getAttribute('aria-busy') === 'false',
  );
  callbacks.get(listener)!({ payload: 'arrangePanels' });
  await until(
    () => !!host.querySelector('dialog[aria-label="Arrange panels"][open]'),
  );
}
function select(label: string, value: string) {
  const control = [...host.querySelectorAll('label')]
    .find((node) => node.textContent?.trim().startsWith(label))!
    .querySelector('select')!;
  control.value = value;
  control.dispatchEvent(new Event('change', { bubbles: true }));
}
async function place(
  panel: string,
  target: string,
  kind = 'swap',
  expectRefusal = false,
) {
  select('Panel', panel);
  select('Drop action', kind);
  await tick();
  host
    .querySelector<HTMLButtonElement>(`button[aria-label$=" at ${target}"]`)!
    .click();
  await tick();
  await until(() => !button('Done')!.disabled);
  const error = host.querySelector('[role="alert"]')?.textContent;
  assert(
    expectRefusal ? !!error : !error,
    `Panel move ${panel} → ${target}: ${error ?? 'no refusal'}`,
  );
}
async function run() {
  await pauseChecks(passed);
  await ownerChecks(passed);
  let app = mount(App, { target: host });
  try {
    await until(
      () =>
        !!listener &&
        !!host.querySelector('main') &&
        host.querySelector('main')?.getAttribute('aria-busy') === 'false',
    );
    const video = host.querySelector<HTMLElement>('.video-space')!;
    const waveform = host.querySelector<HTMLElement>(
      '[aria-label="Waveform"]',
    )!;
    const editor = host.querySelector('[aria-label="Subtitle text"]');
    delayHide = true;
    callbacks.get(listener)!({ payload: 'arrangePanels' });
    await until(() => holdHide !== null);
    assert(
      !host.querySelector('dialog[aria-label="Arrange panels"]'),
      'No drag UI before native hide acknowledgment',
    );
    delayHide = false;
    holdHide!();
    holdHide = null;
    await until(() => !!button('Done'));
    assert(occluded, 'Native video hidden throughout arrange mode');
    passed('arrange mode waits for HideVideo before presenting drag controls');
    await place('waveform', 'Video');
    assert(
      video.getBoundingClientRect().x < waveform.getBoundingClientRect().x,
      `Waveform and video actually swap slots: ${video.getBoundingClientRect().x}/${waveform.getBoundingClientRect().x}; ${host.querySelector('[role=alert]')?.textContent}`,
    );
    assert(
      host.querySelector('[aria-label="Subtitle text"]') === editor &&
        host.querySelector('.video-space') === video,
      'Owners remain mounted',
    );
    assert(occluded, 'Nested layout restore retains modal occlusion');
    await until(() => !!stored.custom && saves > 0);
    passed(
      'slot swaps move mounted panels and retain native occlusion while saving user preferences',
    );
    host.dataset.awaitingDrag = 'true';
    await until(() => stored.custom?.rows[0]?.panels[0] === 'waveform');
    delete host.dataset.awaitingDrag;
    assert(occluded, 'Drag drop retains native hiding');
    await place('waveform', 'Video');
    await until(() => stored.custom?.rows[0]?.panels[0] === 'video');
    passed(
      'real pointer drag uses the same protected slot move as keyboard controls',
    );
    const before = JSON.stringify(stored.custom);
    await place('transport', 'Cue editor', 'beside', true);
    assert(
      host.querySelector('[role="alert"]')?.textContent?.includes('width'),
      'Invalid minimum width has an actionable reason',
    );
    assert(
      JSON.stringify(stored.custom) === before,
      'Rejected move preserves prior layout',
    );
    passed('minimum-size refusal leaves the previous arrangement intact');
    select('Panel', 'video');
    await tick();
    button('Hide selected panel')!.click();
    await until(
      () =>
        !button('Done')!.disabled && video.getBoundingClientRect().width === 0,
    );
    assert(
      getComputedStyle(video).boxShadow !== 'none',
      'Hidden video retains window background',
    );
    assert(occluded, 'Native video stays hidden during arrange');
    await until(
      () =>
        nativeBounds?.kind === 'layout' &&
        nativeBounds.width === 1 &&
        nativeBounds.visible === false,
    );
    passed(
      'hiding Video sends visible:false with valid native dimensions for its zero-size placeholder',
    );
    await place('video', 'Waveform', 'beside');
    assert(
      video.getBoundingClientRect().width > 0,
      'Hidden panel can return beside another',
    );
    passed(
      'hide and restore keep the video placeholder and background owner alive',
    );
    button('Done')!.click();
    await until(
      () =>
        !host.querySelector('dialog[open]') && !occluded && boundsBeforeRestore,
    );
    await until(() => !!stored.custom?.rows[0]?.panels.includes('video'));
    const saved = JSON.stringify(stored.custom);
    media.loaded = false;
    await unmount(app);
    app = mount(App, { target: host });
    await until(
      () =>
        !!host.querySelector('main') &&
        host.querySelector('main')?.getAttribute('aria-busy') === 'false' &&
        !!host.querySelector('.workspace.custom'),
    );
    assert(JSON.stringify(stored.custom) === saved, 'User arrangement reloads');
    passed(
      'Done restores acknowledged native bounds and custom layout survives remount',
    );
    await openArrange();
    button('Reset arrangement')!.click();
    await until(
      () =>
        !button('Done')!.disabled && !host.querySelector('.workspace.custom'),
    );
    host
      .querySelector('dialog')!
      .dispatchEvent(new Event('cancel', { cancelable: true }));
    await until(() => !host.querySelector('dialog[open]') && !occluded);
    await until(() => !stored.custom);
    passed(
      'reset and Escape restore the standard preset and release native occlusion',
    );
    callbacks.get(listener)!({ payload: 'saveProject' });
    await until(
      () =>
        projectSaves.length === 1 &&
        host.querySelector('main')?.getAttribute('aria-busy') === 'false',
    );
    await openArrange();
    const remember = host.querySelector<HTMLInputElement>(
      'dialog input[type="checkbox"]',
    )!;
    // Use checkbox activation and await the owner's rendered scope. Setting the
    // DOM property and firing change can run before the project control is ready.
    await until(() => !remember.disabled);
    remember.click();
    await until(
      () =>
        !remember.disabled &&
        host.textContent!.includes(
          'Project layout changes stay in this workspace',
        ),
    );
    const userWrites = saves;
    await place('waveform', 'Video');
    await until(
      () =>
        !!host.querySelector('.workspace.custom') &&
        host.querySelector('.schematic .tiles button')?.textContent === 'Video',
    );
    snapshot.dirty = true;
    documentActions.length = 0;
    button('Save project layout')!.click();
    await until(() => projectSaves.length === 2 && !button('Done')!.disabled);
    assert(
      projectSaves[1]!.options.layout?.custom?.rows[0]?.panels[0] === 'video',
      `Project save contains arrangement: ${JSON.stringify(projectSaves[1]!.options.layout)}`,
    );
    assert(
      !documentActions.some((kind) => kind === 'save' || kind === 'saveAs') &&
        snapshot.dirty,
      'Layout save does not save subtitle bytes',
    );
    assert(
      saves === userWrites,
      'Project arrangement does not write user preferences',
    );
    passed(
      'Remember and explicit project save persist only the project snapshot, preserving dirty subtitle bytes',
    );
    await until(() => !remember.disabled && remember.checked);
    remember.click();
    await until(
      () =>
        !button('Done')!.disabled && !host.querySelector('.workspace.custom'),
    );
    button('Save project layout')!.click();
    await until(() => projectSaves.length === 3 && !button('Done')!.disabled);
    assert(
      projectSaves[2]!.options.layout === null,
      'Removing override is persisted explicitly',
    );
    button('Done')!.click();
    await until(() => !host.querySelector('dialog[open]') && !occluded);
    passed(
      'removing Remember restores user placement and saves a null project override',
    );
    heading.textContent = 'PASS: all 17 arrange panel checks';
  } finally {
    await unmount(app);
  }
}
run().catch((error) => {
  heading.textContent = 'FAIL: ' + String(error);
  console.error(error);
});
