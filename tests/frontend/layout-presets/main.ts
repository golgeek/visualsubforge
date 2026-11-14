import { emptyQuality } from '../empty-quality';
import { mount, tick, unmount } from 'svelte';
import { preferences as rustPreferences } from '../ipc-contract-samples';
import App from '../../../src/App.svelte';
import {
  defaultPreferences,
  emptyDocument,
  emptyStatus,
  type MediaAction,
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
let nativeBounds: MediaAction | null = null;
let holdHide: (() => void) | null = null;
let delayHide = false;
let boundsBeforeRestore = false;
let rejectHide = false;
let rejectRestoreAfterHide = false;
let rejectRestore = false;
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
        action?: MediaAction;
        handler?: number;
        layout?: Preferences['layout'];
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
            if (args.action.occluded && rejectHide) {
              rejectHide = false;
              throw new Error('Injected layout hide failure');
            }
            if (args.action.occluded && rejectRestoreAfterHide) {
              rejectRestoreAfterHide = false;
              rejectRestore = true;
            } else if (!args.action.occluded && rejectRestore) {
              rejectRestore = false;
              throw new Error('Injected layout restore failure');
            }
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
async function run() {
  let app = mount(App, { target: host });
  try {
    await until(
      () => !!listener && !!button('Layout') && !button('Layout')!.disabled,
    );
    for (const key of [
      'timing',
      'editorAbove',
      'review',
      'translation',
      'bigPicture',
    ] as const) {
      for (const field of ['topPct', 'editorPct', 'wavePct'] as const)
        assert(
          defaultPreferences.layout[key][field] ===
            rustPreferences.layout[key][field],
          'Frontend layout defaults match Rust',
        );
    }
    const capacity = Number(
      new URLSearchParams(location.search).get('capacity'),
    );
    if (capacity) {
      await until(
        () =>
          host.querySelector('.cue-list')!.clientHeight >=
          capacity *
            host.querySelector('.cue-row')!.getBoundingClientRect().height,
      );
      const text = host.querySelector<HTMLTextAreaElement>(
        '[aria-label="Subtitle text"]',
      )!;
      const css = getComputedStyle(text);
      assert(
        (text.clientHeight -
          parseFloat(css.paddingTop) -
          parseFloat(css.paddingBottom)) /
          parseFloat(css.lineHeight) >=
          2,
        'Two editor lines survive native chrome budget',
      );
      passed(
        'Timing preserves requested rows and two editor lines in the smaller hosted client area',
      );
    }
    const waveform = host.querySelector('[aria-label="Waveform"]');
    const editor = host.querySelector('[aria-label="Subtitle text"]');
    const video = host.querySelector('.video-space');
    for (const [id, label] of [
      ['editorAbove', 'Editor above list'],
      ['review', 'Review'],
      ['translation', 'Translation'],
      ['bigPicture', 'Big picture'],
      ['timing', 'Timing'],
    ]) {
      boundsBeforeRestore = false;
      await choose(label!);
      assert(
        host.querySelector('.workspace')?.getAttribute('data-preset') === id,
        'Preset selected: ' + label,
      );
      assert(
        host.querySelector('[aria-label="Waveform"]') === waveform &&
          host.querySelector('[aria-label="Subtitle text"]') === editor &&
          host.querySelector('.video-space') === video,
        'Mounted regions survive preset switching',
      );
      assert(
        boundsBeforeRestore,
        'Native bounds acknowledged before restore: ' + label,
      );
      if (id === 'editorAbove')
        assert(
          editor!.getBoundingClientRect().top <
            host.querySelector('.cue-list')!.getBoundingClientRect().top,
          'Editor above cues',
        );
      if (id === 'review')
        assert(!!host.querySelector('.problems'), 'Review dock appears');
      if (id === 'translation')
        assert(
          !!host.querySelector('.translation'),
          'Translation dock appears',
        );
    }
    passed(
      'all five presets keep mounted owners and acknowledge native bounds before restoring video',
    );
    for (const failure of ['hide', 'restore'] as const) {
      await choose('Timing');
      button('Layout')!.click();
      await until(
        () =>
          !!host.querySelector('dialog[aria-label="Workspace layout"][open]'),
      );
      if (failure === 'hide') rejectHide = true;
      else rejectRestoreAfterHide = true;
      button('Review')!.click();
      await until(
        () =>
          !!host.textContent?.includes(`Injected layout ${failure} failure`) &&
          !host.querySelector('dialog[open]') &&
          !button('Layout')!.disabled &&
          !occluded,
      );
      assert(
        host.querySelector('.workspace')?.getAttribute('data-preset') ===
          (failure === 'hide' ? 'timing' : 'review'),
        'Failed hide preserves preset; failed restore does not replay its change',
      );
      assert(
        !!host.querySelector('.problems') === (failure === 'restore'),
        'Dock changes only after successful hide',
      );
      assert(
        boundsBeforeRestore,
        'Recovery acknowledges current native bounds',
      );
      await choose('Big picture');
      assert(
        host.querySelector('.workspace')?.getAttribute('data-preset') ===
          'bigPicture',
        'Subsequent preset switch works after native failure',
      );
      passed(
        `Layout ${failure} failure reports its error and recovers native visibility and workspace gates`,
      );
    }
    await choose('Timing');
    const splitter = host.querySelector<HTMLButtonElement>(
      '[aria-label="Resize waveform and video"]',
    )!;
    const width = video!.getBoundingClientRect().width;
    delayHide = true;
    splitter.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'ArrowLeft',
        bubbles: true,
        cancelable: true,
      }),
    );
    await until(() => holdHide !== null);
    assert(
      video!.getBoundingClientRect().width === width,
      'No layout mutation before native hide acknowledgment',
    );
    delayHide = false;
    holdHide!();
    holdHide = null;
    await until(
      () =>
        video!.getBoundingClientRect().width > width &&
        !button('Layout')!.disabled,
    );
    await until(() => stored.timing.wavePct === 63 && saves > 0);
    await choose('Big picture');
    assert(stored.timing.wavePct === 63, 'Other preset retains timing sizes');
    await choose('Timing');
    assert(
      Math.abs(video!.getBoundingClientRect().width - width) > 1,
      'Timing restores its resized pane',
    );
    await until(() => stored.active === 'timing');
    passed(
      'splitter waits for hiding and sizes persist independently for each preset',
    );
    await unmount(app);
    app = mount(App, { target: host });
    await until(
      () => !!button('Layout') && !button('Layout')!.disabled && !occluded,
    );
    const reloaded = host
      .querySelector('.video-space')!
      .getBoundingClientRect().width;
    assert(reloaded > width, 'Saved size restored after workspace remount');
    passed(
      'saved layout restores through startup without changing accepted subtitles',
    );
    callbacks.get(listener)!({ payload: 'resetLayout' });
    await until(
      () => stored.timing.wavePct === 65 && !button('Layout')!.disabled,
    );
    assert(stored.active === 'timing', 'Reset keeps the active preset');
    host
      .querySelector<HTMLButtonElement>(
        '[aria-label="Move editor above list"]',
      )!
      .click();
    await until(
      () =>
        host.querySelector('.workspace')?.getAttribute('data-preset') ===
          'editorAbove' && !button('Layout')!.disabled,
    );
    passed(
      'reset preserves the active preset and the editor arrow uses protected slot switching',
    );
    heading.textContent = `PASS: all ${capacity ? 7 : 6} layout preset checks`;
  } finally {
    await unmount(app);
  }
}
run().catch((error) => {
  heading.textContent = 'FAIL: ' + String(error);
  console.error(error);
});
