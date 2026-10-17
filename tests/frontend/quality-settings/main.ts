import { mount, unmount, tick } from 'svelte';
import SettingsDialog from '../../../src/lib/SettingsDialog.svelte';
import {
  defaultPreferences,
  type Preferences,
  type QualityProfile,
  type QualitySettingsData,
} from '../../../src/lib/client';
import { builtinQualityProfiles } from '../../../src/lib/generated/quality-defaults';

const host = document.querySelector('#test')!;
const initial = structuredClone(defaultPreferences);
let accepted = structuredClone(initial);
let current = structuredClone(initial);
let epoch = 1;
let failure = false;
let importFailure = false;
let path: string | null = '/profile.vsfpreset';
let exported: QualityProfile | null = null;
const applied: Preferences[] = [];
let closed = 0;
let reads: Promise<QualitySettingsData> | null = null;
const input = structuredClone(initial);
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
        case 'preferences_save':
          if (failure) throw new Error('Cannot save settings');
          if (args.epoch !== epoch)
            throw new Error('Workspace settings changed; reopen Settings');
          accepted = structuredClone(args.preferences!);
          current = structuredClone(accepted);
          epoch++;
          return;
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
  const deadline = performance.now() + 5000;
  await tick();
  while (!predicate()) {
    if (performance.now() > deadline)
      throw new Error('Timed out waiting for Settings');
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}
function button(name: string): HTMLButtonElement {
  const button = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === name,
  );
  if (!button) throw new Error(`Missing button ${name}`);
  return button;
}
function field(label: string): HTMLInputElement {
  const element = [...host.querySelectorAll('label')]
    .find((e) => e.textContent?.trim().startsWith(label))
    ?.querySelector('input');
  if (!element) throw new Error(`Missing field ${label}`);
  return element;
}
function setField(label: string, value: string, event = 'change') {
  const element = field(label);
  element.value = value;
  element.dispatchEvent(new Event(event, { bubbles: true }));
}
async function open() {
  if (app) await unmount(app);
  app = mount(SettingsDialog, {
    target: host,
    props: {
      preferences: input,
      timing: initial.timing,
      onapply: (value: Preferences) => {
        applied.push(value);
      },
      onclose: () => {
        closed++;
      },
    },
  });
  if (!reads) await until(() => !button('Apply settings').disabled);
  else await tick();
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

async function run() {
  current.quality = structuredClone(builtinQualityProfiles[1]!);
  current.timing = { ...initial.timing, ...current.quality.timing };
  await open();
  check(
    selected() === 'sw3-uk-vo-subset' && field('Between cues').value === '160',
    'Settings loads the accepted project policy instead of global defaults',
  );
  check(
    host.textContent!.includes('Partial coverage') &&
      host.textContent!.includes('Typography'),
    'Partial profile exposes unsupported rules before Apply',
  );
  choose('vsf-default');
  await tick();
  check(
    field('Between cues').value === '80' &&
      current.quality.id === 'sw3-uk-vo-subset',
    'Profile selection updates only draft timing thresholds',
  );
  button('Cancel').click();
  await tick();
  check(
    closed === 1 &&
      applied.length === 0 &&
      JSON.stringify(input) === JSON.stringify(initial),
    'Cancel preserves caller preferences and accepted rules',
  );

  await open();
  choose('vsf-default');
  await tick();
  button('Customize a copy').click();
  await tick();
  check(
    selected().startsWith('custom-') &&
      !field('Custom profile name').matches(':disabled'),
    'Customization gets a new identity with editable rules',
  );
  setField('Custom profile name', 'My delivery rules', 'input');
  setField('Maximum duration', '6000');
  setField('Maximum CPS', '');
  await tick();
  button('Export quality profile').click();
  await until(() => exported !== null);
  check(
    exported!.name === 'My delivery rules' &&
      exported!.rules.duration.maximumMs === 6000 &&
      exported!.rules.speed.maximumCps === null,
    'Export uses the complete custom draft and nullable speed bounds',
  );
  check(
    accepted.quality.id === initial.quality.id && applied.length === 0,
    'Export does not apply the draft',
  );

  const customId = selected();
  importFailure = true;
  button('Import quality profile').click();
  await until(() => host.textContent!.includes('Invalid quality profile'));
  check(
    selected() === customId &&
      field('Custom profile name').value === 'My delivery rules',
    'Invalid import preserves all draft rules',
  );
  importFailure = false;
  path = null;
  button('Import quality profile').click();
  await until(() => !button('Import quality profile').matches(':disabled'));
  check(selected() === customId, 'Cancelled import preserves the draft');
  path = '/profile.vsfpreset';
  button('Import quality profile').click();
  await until(() => selected() === 'sw3-uk-vo-subset');
  const beforeRules = JSON.stringify(builtinQualityProfiles[1]!.rules);
  button('Import timing preset').click();
  await until(() => field('Between cues').value === '167');
  exported = null;
  button('Export quality profile').click();
  await until(() => exported !== null);
  check(
    JSON.stringify(exported!.rules) === beforeRules &&
      exported!.timing.cueGapMs === 167 &&
      exported!.id !== 'sw3-uk-vo-subset',
    'Version-1 timing import retains quality rules in an explicit custom copy',
  );

  failure = true;
  button('Apply settings').click();
  await until(() => host.textContent!.includes('Cannot save settings'));
  check(
    applied.length === 0 &&
      accepted.quality.id === initial.quality.id &&
      selected() === exported!.id,
    'Failed Apply preserves accepted state and the editable draft',
  );
  failure = false;
  button('Apply settings').click();
  await until(() => applied.length === 1);
  check(
    accepted.quality.timing.cueGapMs === 167 &&
      accepted.timing.cueGapMs === 167 &&
      JSON.stringify(accepted.quality.rules) === beforeRules,
    'Successful Apply accepts one coherent profile and timing snapshot',
  );
  check(
    JSON.stringify(input) === JSON.stringify(initial),
    'Nested draft edits never mutate the original preference projection',
  );

  await open();
  epoch++;
  button('Apply settings').click();
  await until(() => host.textContent!.includes('Workspace settings changed'));
  check(applied.length === 1, 'A stale workspace epoch prevents acceptance');

  let resolve!: (data: QualitySettingsData) => void;
  reads = new Promise((done) => {
    resolve = done;
  });
  await open();
  check(
    button('Apply settings').disabled && !button('Cancel').disabled,
    'Loading disables Apply while cancellation remains available',
  );
  button('Cancel').click();
  await unmount(app!);
  app = null;
  resolve({
    current: { epoch, profile: initial.quality, timing: initial.timing },
    builtins: builtinQualityProfiles,
  });
  await tick();
  await Promise.resolve();
  check(
    host.childElementCount === 0 && applied.length === 1,
    'Late loading responses cannot revive a closed Settings draft',
  );
  const count = document.querySelectorAll('#checks li').length;
  document.querySelector('#result')!.textContent =
    `PASS: all ${count} quality Settings checks`;
}
void run().catch(async (error: unknown) => {
  document.querySelector('#result')!.textContent = `FAIL: ${String(error)}`;
  if (app) await unmount(app);
});
