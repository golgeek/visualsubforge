import { mount, tick, unmount } from 'svelte';
import SettingsDialog from '../../../src/lib/settings/SettingsDialog.svelte';
import { Appearance } from '../../../src/lib/ui/appearance';
import { defaultPreferences, type Preferences } from '../../../src/lib/client';
import '../../../src/lib/ui/tokens.css';
import '../../../src/lib/ui/base.css';

const host = document.querySelector<HTMLElement>('#test')!;
const root = document.documentElement;
let saved = structuredClone(defaultPreferences);
let epoch = 1;
let saves = 0;
let failure = false;
let closed = false;
let systemDark = true;
let listeners = 0;
const callbacks = new Set<() => void>();
const system = {
  get matches() {
    return systemDark;
  },
  addEventListener: (_event: string, listener: () => void) => {
    ++listeners;
    callbacks.add(listener);
  },
  removeEventListener: (_event: string, listener: () => void) => {
    --listeners;
    callbacks.delete(listener);
  },
  change: () => {
    for (const callback of callbacks) callback();
  },
};
const owner = new Appearance(root, system);
owner.apply(saved.appearance);
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    invoke: async (
      command: string,
      args: { preferences?: Preferences; epoch?: number },
    ) => {
      switch (command) {
        case 'preferences_get':
          return structuredClone(saved);
        case 'quality_settings':
          return {
            current: { epoch, profile: saved.quality, timing: saved.timing },
            builtins: [saved.quality],
          };
        case 'language_words':
          return {
            epoch,
            personal: saved.personalWords,
            ignored: { englishUs: [], french: [] },
          };
        case 'preferences_save':
          if (failure) throw new Error('Appearance save rejected');
          if (args.epoch !== epoch) throw new Error('Stale settings');
          saved = JSON.parse(JSON.stringify(args.preferences)) as Preferences;
          ++saves;
          return ++epoch;
        default:
          throw new Error('Unexpected command: ' + command);
      }
    },
  },
});
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const item = document.createElement('li');
  item.textContent = 'PASS: ' + message;
  document.querySelector('#checks')!.append(item);
}
async function until(condition: () => boolean) {
  const deadline = performance.now() + 5000;
  while (!condition()) {
    assert(
      performance.now() < deadline,
      'Appearance did not settle: ' + host.textContent,
    );
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await tick();
}
function button(text: string) {
  return [...host.querySelectorAll('button')].find(
    (item) => item.textContent?.trim() === text,
  )!;
}
async function select(label: string, value: string) {
  const input = host.querySelector<HTMLSelectElement>(
    `[aria-label="${label}"]`,
  )!;
  input.value = value;
  input.dispatchEvent(new Event('change', { bubbles: true }));
  await tick();
}
function open() {
  closed = false;
  return mount(SettingsDialog, {
    target: host,
    props: {
      preferences: structuredClone(saved),
      timing: saved.timing,
      onapply: (value) => owner.apply(value.appearance),
      onclose: () => {
        closed = true;
      },
    },
  });
}
async function run() {
  let dialog = open();
  try {
    await until(
      () =>
        !host.querySelector<HTMLSelectElement>('[aria-label="Theme"]')!
          .disabled,
    );
    button('Appearance').click();
    await tick();
    await select('Theme', 'light');
    await until(() => root.dataset.theme === 'light');
    assert(
      saved.appearance.theme === 'light' && saves === 1,
      'Theme persisted before publication',
    );
    assert(
      getComputedStyle(document.body).backgroundColor === 'rgba(0, 0, 0, 0)',
      'Body stays transparent',
    );
    passed(
      'Light theme saves and applies without reload, preserving the transparent body',
    );
    button('Undo settings change').click();
    await until(() => root.dataset.theme === 'dark');
    assert(saved.appearance.theme === 'dark', 'Undo persisted');
    passed('Settings Undo restores the previous confirmed theme');
    const columns = JSON.stringify(saved.layout);
    const compactPadding = getComputedStyle(button('Close')).paddingTop;
    await select('Control density', 'comfortable');
    await until(() => root.dataset.density === 'comfortable');
    assert(
      getComputedStyle(button('Close')).paddingTop !== compactPadding,
      'Control density changes token spacing',
    );
    assert(
      JSON.stringify(saved.layout) === columns,
      'Per-preset cue rows unchanged',
    );
    passed(
      'Global density changes controls while preserving every preset’s cue-row preferences',
    );
    await select('Theme', 'system');
    await until(() => saved.appearance.theme === 'system');
    systemDark = false;
    system.change();
    await tick();
    assert(
      root.dataset.theme === 'light' && listeners === 1,
      'One system observer follows light',
    );
    systemDark = true;
    system.change();
    await tick();
    assert(root.dataset.theme === 'dark', 'System dark follows without saving');
    passed('System follows color-scheme changes through one listener');
    await select('Theme', 'light');
    await until(() => saved.appearance.theme === 'light');
    systemDark = true;
    system.change();
    await tick();
    assert(
      root.dataset.theme === 'light',
      'Explicit theme ignores system changes',
    );
    button('Close').click();
    await until(() => closed);
    await unmount(dialog);
    dialog = open();
    await until(
      () =>
        !host.querySelector<HTMLSelectElement>('[aria-label="Theme"]')!
          .disabled,
    );
    button('Appearance').click();
    await tick();
    assert(
      host.querySelector<HTMLSelectElement>('[aria-label="Theme"]')!.value ===
        'light' &&
        host.querySelector<HTMLSelectElement>('[aria-label="Control density"]')!
          .value === 'comfortable',
      'Restored persisted settings',
    );
    passed(
      'Explicit theme ignores the system and saved theme/density survive dialog remount',
    );
    failure = true;
    await select('Theme', 'dark');
    await until(() => !!host.textContent?.includes('Appearance save rejected'));
    assert(
      root.dataset.theme === 'light' && saved.appearance.theme === 'light',
      'Rejected preference never becomes effective',
    );
    passed('A failed save preserves the effective theme');
    owner.dispose();
    const theme = root.getAttribute('data-theme');
    systemDark = false;
    system.change();
    owner.apply({ ...saved.appearance, theme: 'dark' });
    assert(
      listeners === 0 && root.getAttribute('data-theme') === theme,
      'Disposal retires observer and late changes',
    );
    passed(
      'Disposal removes the system listener and prevents late publication',
    );
    document.querySelector('#result')!.textContent =
      'PASS: all 7 appearance checks';
  } finally {
    await unmount(dialog);
    owner.dispose();
  }
}
run().catch((error) => {
  document.querySelector('#result')!.textContent = 'FAIL: ' + String(error);
  console.error(error);
});
