import { tick } from 'svelte';
import { SettingsSession } from '../../../src/lib/settings/settings-session.svelte';
import { defaultPreferences, type Preferences } from '../../../src/lib/client';
import { builtinQualityProfiles } from '../../../src/lib/generated/quality-defaults';
const copy = <T>(value: T): T => structuredClone(value);
function check(value: unknown, label: string) {
  if (!value) throw new Error(label);
  const item = document.createElement('li');
  item.textContent = `PASS: ${label}`;
  document.querySelector('#checks')!.append(item);
}
async function until(condition: () => boolean) {
  const deadline = performance.now() + 5000;
  while (!condition()) {
    if (performance.now() > deadline)
      throw new Error('Settings owner timed out');
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await tick();
}
const delay = () => new Promise((resolve) => setTimeout(resolve, 350));
async function run() {
  let saved = copy(defaultPreferences);
  let epoch = 1;
  let calls = 0;
  let active = 0;
  let maximum = 0;
  let blocked = '';
  let failure = false;
  let release: (() => void) | null = null;
  let hold = false;
  const accepted: Preferences[] = [];
  const client = {
    getPreferences: async () => copy(saved),
    getQualitySettings: async () => ({
      current: {
        epoch,
        profile: copy(saved.quality),
        timing: copy(saved.timing),
      },
      builtins: builtinQualityProfiles,
    }),
    getLanguageWords: async () => ({
      epoch,
      personal: copy(saved.personalWords),
      ignored: { englishUs: [], french: [] },
    }),
    savePreferences: async (value: Preferences, captured: number) => {
      calls++;
      maximum = Math.max(maximum, ++active);
      try {
        if (hold)
          await new Promise<void>((resolve) => {
            release = resolve;
          });
        if (failure) throw new Error('Injected settings failure');
        if (captured !== epoch) throw new Error('Stale settings');
        saved = copy(value);
        return ++epoch;
      } finally {
        active--;
      }
    },
  };
  const session = new SettingsSession(
    copy(saved),
    (value) => accepted.push(value),
    () => blocked || null,
    client,
  );
  await session.load();
  check(session.ready && !calls, 'Loading settings is read-only');
  session.draft.autoScanScenes = false;
  void session.changed();
  session.draft.showQualityMetrics = true;
  void session.changed();
  await until(() => accepted.length === 1);
  check(
    calls === 1 && !saved.autoScanScenes && saved.showQualityMetrics,
    'Rapid valid changes coalesce into one coherent confirmed save',
  );
  hold = true;
  session.draft.autoScanScenes = true;
  void session.changed();
  await until(() => release !== null);
  session.draft.sceneDetail = 'reduced';
  void session.changed();
  await delay();
  check(
    calls === 2 && maximum === 1,
    'Input during a held save never creates concurrent mutations',
  );
  hold = false;
  release!();
  release = null;
  await until(() => calls === 3 && !session.saving);
  check(
    saved.autoScanScenes && saved.sceneDetail === 'reduced' && maximum === 1,
    'Latest queued draft saves after the returned epoch advances',
  );
  await session.undo();
  check(
    saved.sceneDetail === 'standard' &&
      saved.autoScanScenes &&
      !session.canUndo,
    'Undo restores the previous confirmed settings checkpoint',
  );
  blocked = 'Invalid numeric field';
  const before = calls;
  session.draft.autoScanScenes = false;
  await session.changed();
  await delay();
  check(
    calls === before &&
      !(await session.flush()) &&
      session.draft.autoScanScenes === false,
    'Invalid raw fields stop automatic and close-triggered saves without discarding the draft',
  );
  blocked = '';
  failure = true;
  // Model the field handler and bubbling form event, followed immediately by
  // Close flushing the draft before the debounce. Neither timer may survive.
  await Promise.all([session.changed(), session.changed()]);
  await session.flush();
  const failed = calls;
  check(
    calls === failed && session.dirty,
    'A failed mutation stops automatic retries',
  );
  failure = false;
  await session.reconcile();
  check(
    calls === failed && !session.uncertain && session.dirty,
    'Read-only reconciliation retains the unsaved draft for explicit retry',
  );
  await session.changed();
  await delay();
  check(
    calls === failed,
    'Late change events after reconciliation cannot automatically replay the failed mutation',
  );
  await session.flush();
  check(
    !saved.autoScanScenes && !session.dirty,
    'Explicit retry uses the reconciled identity',
  );
  hold = true;
  session.draft.autoScanScenes = true;
  void session.changed();
  await until(() => release !== null);
  const notified = accepted.length;
  session.dispose();
  hold = false;
  release!();
  await until(() => active === 0);
  check(
    accepted.length === notified,
    'A late accepted reply cannot publish into disposed Settings',
  );
  let finish!: (
    value: Awaited<ReturnType<typeof client.getQualitySettings>>,
  ) => void;
  const late = new SettingsSession(
    copy(saved),
    () => {
      throw new Error('Disposed load published');
    },
    () => null,
    {
      ...client,
      getQualitySettings: () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    },
  );
  const load = late.load();
  late.dispose();
  finish({
    current: { epoch, profile: saved.quality, timing: saved.timing },
    builtins: builtinQualityProfiles,
  });
  await load;
  check(!late.ready, 'Late initialization cannot revive a disposed owner');
  document.querySelector('#result')!.textContent =
    `PASS: all ${document.querySelectorAll('#checks li').length} live Settings checks`;
}
void run().catch((error) => {
  document.querySelector('#result')!.textContent = `FAIL: ${String(error)}`;
});
