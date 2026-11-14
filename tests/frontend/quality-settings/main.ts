import { mount, unmount, tick } from 'svelte';
import SettingsDialog from '../../../src/lib/settings/SettingsDialog.svelte';
import ActionBrowser from '../../../src/lib/workspace/ActionBrowser.svelte';
import {
  defaultPreferences,
  type Preferences,
  type QualityProfile,
  type QualitySettingsData,
} from '../../../src/lib/client';
import { builtinQualityProfiles } from '../../../src/lib/generated/quality-defaults';
const host = document.querySelector('#test')!;
const initial = structuredClone(defaultPreferences);
const input = structuredClone(initial);
let current = structuredClone(initial);
let epoch = 1;
let failure: 'none' | 'before' | 'after' = 'none';
let importFailure = false;
let path: string | null = '/profile.vsfpreset';
let exported: QualityProfile | null = null;
let saves = 0;
let closed = 0;
const applied: Preferences[] = [];
let reads: Promise<QualitySettingsData> | null = null;
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    invoke: async (
      command: string,
      args: {
        preferences?: Preferences;
        epoch?: number;
        profile?: QualityProfile;
      },
    ) => {
      switch (command) {
        case 'preferences_get':
          return structuredClone(current);
        case 'language_words':
          return {
            epoch,
            personal: structuredClone(current.personalWords),
            ignored: { englishUs: [], french: [] },
          };
        case 'language_notices':
          return {
            english: 'English license notice fixture',
            french: 'French license notice fixture',
            mpl: 'MPL-2.0 fixture',
            sources: 'Pinned source fixture',
          };
        case 'quality_settings':
          return (
            reads ?? {
              current: {
                epoch,
                profile: structuredClone(current.quality),
                timing: structuredClone(current.timing),
              },
              builtins: structuredClone(builtinQualityProfiles),
            }
          );
        case 'choose_file':
          return path;
        case 'quality_profile_open':
          if (importFailure) throw new Error('Invalid quality profile');
          return structuredClone(builtinQualityProfiles[1]);
        case 'quality_profile_save':
          exported = structuredClone(args.profile!);
          return;
        case 'timing_preset_open':
          return {
            preset: {
              version: 1,
              name: 'Timing only',
              timing: { ...initial.timing, cueGapMs: 167 },
            },
            warnings: [],
          };
        case 'preferences_save': {
          saves++;
          if (failure === 'before') throw new Error('Cannot save settings');
          if (args.epoch !== epoch)
            throw new Error('Workspace settings changed; reopen Settings');
          current = structuredClone(args.preferences!);
          epoch++;
          if (failure === 'after') throw new Error('Lost settings reply');
          return epoch;
        }
        default:
          throw new Error(`Unexpected command ${command}`);
      }
    },
  },
});
let app: ReturnType<typeof mount> | null = null;
function check(condition: unknown, label: string) {
  if (!condition) throw new Error(label);
  const row = document.createElement('li');
  row.textContent = `PASS: ${label}`;
  document.querySelector('#checks')!.append(row);
}
async function until(predicate: () => boolean) {
  const end = performance.now() + 5000;
  await tick();
  while (!predicate()) {
    if (performance.now() > end)
      throw new Error(
        `Timed out waiting for Settings: ${host.querySelector('footer')?.textContent}`,
      );
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}
const pause = () => new Promise((resolve) => setTimeout(resolve, 350));
function button(name: string): HTMLButtonElement {
  const value = [...host.querySelectorAll('button')].find(
    (button) => button.textContent?.trim() === name,
  );
  if (!value) throw new Error(`Missing button ${name}`);
  return value;
}
async function page(name: string) {
  button(name).click();
  await tick();
}
function field(label: string): HTMLInputElement {
  const value = [...host.querySelectorAll('label')]
    .find((element) => element.textContent?.trim().startsWith(label))
    ?.querySelector('input');
  if (!value) throw new Error(`Missing field ${label}`);
  return value;
}
function set(label: string, value: string, event = 'change') {
  const element = field(label);
  element.value = value;
  element.dispatchEvent(new Event(event, { bubbles: true }));
}
function choose(id: string) {
  const select = host.querySelector<HTMLSelectElement>(
    'select[aria-label="Quality profile"]',
  )!;
  select.value = id;
  select.dispatchEvent(new Event('change', { bubbles: true }));
}
function selected() {
  return host.querySelector<HTMLSelectElement>(
    'select[aria-label="Quality profile"]',
  )!.value;
}
async function open() {
  if (app) await unmount(app);
  app = mount(SettingsDialog, {
    target: host,
    props: {
      preferences: structuredClone(current),
      timing: current.timing,
      onapply: (value) => applied.push(value),
      onclose: () => {
        closed++;
      },
    },
  });
  if (!reads)
    await until(
      () =>
        !host
          .querySelector<HTMLSelectElement>(
            'select[aria-label="Quality profile"]',
          )!
          .matches(':disabled'),
    );
  else await tick();
}
async function run() {
  current.quality = structuredClone(builtinQualityProfiles[1]!);
  current.timing = { ...initial.timing, ...current.quality.timing };
  await open();
  await page('Quality profile');
  check(
    selected() === 'sw3-uk-vo-subset' && field('Between cues').value === '160',
    'Settings loads the accepted project policy instead of global defaults',
  );
  check(
    host.textContent!.includes('Partial coverage'),
    'Partial profile retains unsupported coverage information',
  );
  const search = host.querySelector<HTMLInputElement>(
    '[aria-label="Search settings"]',
  )!;
  search.value = 'snapping';
  search.dispatchEvent(new Event('input', { bubbles: true }));
  await tick();
  check(
    host.querySelector('[aria-current="page"]')?.textContent ===
      'Timing and snapping',
    'Settings search navigates to matching sidebar pages',
  );
  search.value = '';
  search.dispatchEvent(new Event('input', { bubbles: true }));
  await tick();
  await page('Quality profile');
  choose('vsf-default');
  await until(() => current.quality.id === 'vsf-default');
  check(
    !!host.querySelector('dialog[open]') && current.timing.cueGapMs === 80,
    'Valid changes apply immediately and keep Settings open',
  );
  button('Undo settings change').click();
  await until(() => current.quality.id === 'sw3-uk-vo-subset');
  check(
    current.timing.cueGapMs === 160 && button('Undo settings change').disabled,
    'Undo restores the previous confirmed profile and timing together',
  );
  choose('vsf-default');
  await tick();
  button('Customize a copy').click();
  await tick();
  check(
    selected().startsWith('custom-') &&
      !field('Custom profile name').matches(':disabled'),
    'Customization creates a separate editable profile',
  );
  set('Custom profile name', 'My delivery rules', 'input');
  set('Maximum duration', '6000');
  set('Maximum CPS', '');
  await until(() => current.quality.name === 'My delivery rules');
  check(
    current.quality.rules.duration.maximumMs === 6000 &&
      current.quality.rules.speed.maximumCps === null,
    'Debounced save keeps complete custom rules and nullable speed bounds coherent',
  );
  const beforeInvalid = saves;
  set('Minimum duration', '0');
  await pause();
  check(
    field('Minimum duration').value === '0' &&
      saves === beforeInvalid &&
      host.textContent!.includes('not saved'),
    'Invalid input remains visible with a reason and is not saved',
  );
  await page('General');
  await page('Quality profile');
  check(
    field('Minimum duration').value === '0',
    'Invalid raw fields survive sidebar navigation',
  );
  set('Minimum duration', '900');
  await until(() => current.quality.rules.duration.minimumMs === 900);
  const beforeExport = saves;
  button('Export quality profile').click();
  await until(() => exported !== null);
  check(
    exported!.name === 'My delivery rules' &&
      exported!.rules.speed.maximumCps === null &&
      saves === beforeExport,
    'Export preserves complete rules without adding a settings mutation',
  );
  importFailure = true;
  button('Import quality profile').click();
  await until(() => host.textContent!.includes('Invalid quality profile'));
  check(
    selected() === current.quality.id,
    'Invalid import preserves accepted rules and the editable draft',
  );
  importFailure = false;
  path = null;
  button('Import quality profile').click();
  await until(() => !button('Import quality profile').matches(':disabled'));
  check(
    selected() === current.quality.id,
    'Cancelled import leaves settings intact',
  );
  path = '/profile.vsfpreset';
  button('Import quality profile').click();
  await until(() => current.quality.id === 'sw3-uk-vo-subset');
  const rules = JSON.stringify(current.quality.rules);
  await page('Timing and snapping');
  button('Import timing preset').click();
  await until(() => current.timing.cueGapMs === 167);
  check(
    JSON.stringify(current.quality.rules) === rules &&
      current.quality.id !== 'sw3-uk-vo-subset',
    'Timing-only import preserves quality rules in a custom profile',
  );
  failure = 'before';
  const old = current.timing.cueGapMs;
  set('Between cues', '200');
  await until(() => host.textContent!.includes('Cannot save settings'));
  const failedSaves = saves;
  await pause();
  check(
    current.timing.cueGapMs === old &&
      field('Between cues').value === '200' &&
      saves === failedSaves,
    'Failed save retains the draft and never retries a mutation automatically',
  );
  failure = 'none';
  button('Check saved settings').click();
  await until(() => host.textContent!.includes('draft is retained'));
  check(
    saves === failedSaves,
    'Read-only reconciliation does not replay the failed save',
  );
  button('Retry save').click();
  await until(() => current.timing.cueGapMs === 200);
  check(
    current.quality.timing.cueGapMs === 200,
    'Explicit retry uses the confirmed epoch and one coherent snapshot',
  );
  failure = 'after';
  set('Between cues', '210');
  await until(() => host.textContent!.includes('Lost settings reply'));
  const lostSaves = saves;
  failure = 'none';
  button('Check saved settings').click();
  await until(() => host.textContent!.includes('No retry was sent'));
  check(
    current.timing.cueGapMs === 210 && saves === lostSaves,
    'Lost accepted reply is reconciled without a duplicate mutation',
  );
  button('Undo settings change').click();
  await until(() => current.timing.cueGapMs === 200);
  check(
    current.quality.timing.cueGapMs === 200,
    'Undo after reconciliation restores the prior confirmed policy',
  );
  epoch++;
  set('Between cues', '220');
  await until(() => host.textContent!.includes('Workspace settings changed'));
  check(
    current.timing.cueGapMs === 200,
    'A stale policy epoch refuses acceptance',
  );
  button('Check saved settings').click();
  await until(() => host.textContent!.includes('draft is retained'));
  button('Retry save').click();
  await until(() => current.timing.cueGapMs === 220);
  await page('Spelling and language');
  button('Customize a copy').click();
  await tick();
  const dictionary = host.querySelector<HTMLSelectElement>(
    '[aria-label="Language dictionary"]',
  )!;
  dictionary.value = 'fr';
  dictionary.dispatchEvent(new Event('change', { bubbles: true }));
  for (const label of [
    'Check spelling',
    'Check typography',
    'Check conservative sentence capitalization',
    'Check existing dialogue markers',
  ]) {
    const element = field(label);
    if (!element.checked) element.click();
  }
  const personal = host.querySelector<HTMLTextAreaElement>(
    '[aria-label="French personal words"]',
  )!;
  personal.value = 'Élodiex\nFanword';
  personal.dispatchEvent(new Event('input', { bubbles: true }));
  await until(() => current.personalWords.french.length === 2);
  check(
    current.quality.languageTools?.dictionary === 'fr' &&
      current.quality.languageTools.spelling &&
      current.quality.languageTools.dialogueMarkers,
    'French policy and exact personal Unicode words save together',
  );
  button('Read bundled license notices').click();
  await until(() =>
    host.textContent!.includes('English license notice fixture'),
  );
  check(
    host.textContent!.includes('French license notice fixture') &&
      host.textContent!.includes('MPL-2.0 fixture'),
    'Bundled dictionary notices remain available',
  );
  check(
    JSON.stringify(input) === JSON.stringify(initial),
    'Nested settings operations preserve the original preference projection',
  );
  await page('Shortcuts');
  const preset = host.querySelector<HTMLSelectElement>(
    '[aria-label="Shortcut preset"]',
  )!;
  preset.value = 'visualSubSyncClassic';
  preset.dispatchEvent(new Event('change', { bubbles: true }));
  await until(() => current.shortcuts.preset === 'visualSubSyncClassic');
  check(
    current.shortcuts.bindings.some(
      (binding) => binding.action === 'play' && binding.key === 'F1',
    ),
    'Shortcut preset applies and persists through the same confirmed settings owner',
  );
  const split = host.querySelector<HTMLInputElement>(
    '[aria-label="Split at Playhead — Everywhere"]',
  )!;
  const beforeConflict = saves;
  split.value = 'F1';
  split.dispatchEvent(new Event('change', { bubbles: true }));
  await until(() => host.textContent!.includes('conflicts with'));
  await new Promise((resolve) => setTimeout(resolve, 350));
  check(
    saves === beforeConflict,
    'Overlapping shortcut conflict keeps the draft unsaved',
  );
  const record = host.querySelector<HTMLButtonElement>(
    '[aria-label="Record Split at Playhead — Everywhere"]',
  )!;
  record.click();
  split.dispatchEvent(
    new KeyboardEvent('keydown', {
      key: 'l',
      ctrlKey: true,
      shiftKey: true,
      bubbles: true,
    }),
  );
  await until(() =>
    current.shortcuts.bindings.some(
      (b) =>
        b.action === 'splitCue' &&
        b.mode === 'global' &&
        b.key === 'CmdOrCtrl+Shift+L',
    ),
  );
  check(
    current.shortcuts.preset === 'custom' &&
      split.value === 'CmdOrCtrl+Shift+L',
    'Recording a valid chord resolves conflict and persists a custom binding',
  );
  button('Close').click();
  await until(() => closed === 1);
  await unmount(app!);
  app = null;
  const context = {
    ready: true,
    busy: false,
    document: false,
    media: false,
    available: false,
    cue: false,
    cues: false,
    draft: false,
    styled: false,
    webvtt: false,
    undo: false,
    redo: false,
    range: false,
    scenesRunning: false,
    scenesPending: false,
    previousScene: false,
    nextScene: false,
    problems: false,
    previousProblem: false,
    nextProblem: false,
    operations: {},
  };
  app = mount(ActionBrowser, {
    target: host,
    props: {
      mode: 'shortcuts',
      shortcuts: current.shortcuts,
      context,
      cues: [],
      onaction: async () => {},
      oncue: async () => {},
      onclose: async () => {},
    },
  });
  await tick();
  check(
    [...host.querySelectorAll('.shortcut-row')].some(
      (row) =>
        row.textContent?.includes('Play / Pause') &&
        row.textContent?.includes('F1'),
    ),
    'Shortcut sheet displays the selected preset',
  );
  await unmount(app);
  app = null;
  let resolve!: (value: QualitySettingsData) => void;
  reads = new Promise((done) => {
    resolve = done;
  });
  const beforeLate = applied.length;
  await open();
  button('Close').click();
  await unmount(app!);
  app = null;
  resolve({
    current: { epoch, profile: current.quality, timing: current.timing },
    builtins: builtinQualityProfiles,
  });
  await tick();
  await Promise.resolve();
  check(
    host.childElementCount === 0 && applied.length === beforeLate,
    'Late loading responses cannot revive closed Settings',
  );
  document.querySelector('#result')!.textContent =
    `PASS: all ${document.querySelectorAll('#checks li').length} quality Settings checks`;
}
void run().catch(async (error) => {
  document.querySelector('#result')!.textContent = `FAIL: ${String(error)}`;
  if (app) await unmount(app);
});
