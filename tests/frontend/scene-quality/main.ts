import episode from './timing-episode.json';
import { emptyQuality } from '../empty-quality';
import { chooseCommand } from '../command-discovery';
import { mount, unmount, tick } from 'svelte';
import App from '../../../src/App.svelte';
import {
  emptyDocument,
  emptyStatus,
  defaultPreferences,
  type DocumentSnapshot,
  type DocumentAction,
  type TimingFixView,
  type TimingFixInput,
  type ReviewSelection,
} from '../../../src/lib/client';
import {
  statisticsView as stats,
  timingFixView as sample,
} from '../ipc-contract-samples';
import { lifecycle } from './lifecycle';
const activityTrial = new URLSearchParams(location.search).has('activity');
const episodeTrial = new URLSearchParams(location.search).has('episode');
const incompleteTrial = new URLSearchParams(location.search).has('incomplete');
const policyTrial = new URLSearchParams(location.search).has('policy');
const host = document.querySelector<HTMLElement>('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
let doc: DocumentSnapshot = {
  ...emptyDocument,
  loaded: true,
  generation: sample.key.generation,
  revision: sample.key.revision,
  path: '/fixture.srt',
  cues: [
    { id: 1, startMs: 1000, endMs: 3000, text: '{\\i1}First é 😀' },
    { id: 2, startMs: 4050, endMs: 6050, text: 'Second' },
    { id: 3, startMs: 8000, endMs: 10000, text: '{\\k20}Timed' },
  ],
};
if (episodeTrial) doc.cues = structuredClone(episode.cues);
const navDelay = new URLSearchParams(location.search).get('navigation');
if (navDelay !== null)
  doc.cues = Array.from({ length: 160 }, (_, i) => ({
    id: i + 1,
    startMs: i * 2000,
    endMs: i * 2000 + 1500,
    text: 'Cue ' + (i + 1),
  }));
const original = structuredClone(doc);
let statisticsRevision = doc.revision;
let activeScope: number[] | null = null;
let view: TimingFixView = structuredClone(sample);
let before = structuredClone(doc);
let accepts = 0,
  starts = 0,
  edits = 0;
let failApply = false,
  failAccept = false;
let heldStart: Promise<number> | null = null;
const cancelled: number[] = [];
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  output.append(row);
}
function button(text: string) {
  const b = [
    ...host.querySelectorAll<HTMLButtonElement>(
      'button:not(dialog:not([open]) button)',
    ),
  ].find((b) => b.textContent?.trim() === text);
  if (!b) throw new Error(`Missing ${text}`);
  return b;
}
async function until(check: () => boolean) {
  await tick();
  const end = performance.now() + 6000;
  while (!check()) {
    assert(performance.now() < end, 'Timed out: ' + host.textContent);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
function type(text: string) {
  const input = host.querySelector<HTMLTextAreaElement>(
    '[aria-label="Subtitle text"]',
  )!;
  input.value = text;
  input.dispatchEvent(new Event('input', { bubbles: true }));
}
async function open() {
  await until(
    () => host.querySelector('main')?.getAttribute('aria-busy') === 'false',
  );
  await chooseCommand(host, 'snapShots');
}
Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
  configurable: true,
  value: { unregisterListener: () => {} },
});
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    transformCallback: () => 1,
    unregisterCallback: () => {},
    invoke: async (
      command: string,
      args: {
        action?: DocumentAction;
        options?: TimingFixInput['options'];
        input?: TimingFixInput;
        section?: 'changes' | 'manual';
        id?: number;
        offset?: number;
        limit?: number;
        selection?: ReviewSelection;
        previewIds?: number[];
        acknowledgeNoMedia?: boolean;
      },
    ) => {
      switch (command) {
        case 'recent_list':
        case 'recent_capture':
        case 'recent_forget':
          return [];
        case 'plugin:event|listen':
          return 1;
        case 'plugin:event|unlisten':
        case 'update_menu':
          return;
        case 'preferences_get':
          return {
            ...structuredClone(defaultPreferences),
            timingRepair:
              JSON.parse(
                localStorage.getItem('test-timing-options') ?? 'null',
              ) ?? defaultPreferences.timingRepair,
          };
        case 'timing_repair_options_save':
          if (sessionStorage.getItem('fail-policy')) {
            sessionStorage.setItem(
              'failed-policy-writes',
              String(
                Number(sessionStorage.getItem('failed-policy-writes')) + 1,
              ),
            );
            throw new Error('Injected preference write failure');
          }
          localStorage.setItem(
            'test-timing-options',
            JSON.stringify(args.options),
          );
          return;

        case 'recovery_status':
          return { entries: [], error: null };
        case 'media_action':
          if (activityTrial || navDelay !== null) {
            const action = args.action as { kind: string };
            if (action.kind === 'seek') {
              host.dataset.seeks = String(Number(host.dataset.seeks ?? 0) + 1);
              host.dataset.seek = 'pending';
              await new Promise((resolve) =>
                setTimeout(resolve, Number(navDelay ?? host.dataset.delay)),
              );
              host.dataset.seek = 'done';
            }
            return {
              ...emptyStatus,
              available: true,
              loaded: true,
              paused: !activityTrial,
              session: 1,
              positionSeconds: 0,
              durationSeconds: 340,
            };
          }
          return structuredClone(emptyStatus);
        case 'statistics_start':
          statisticsRevision = doc.revision;
          return 1;
        case 'statistics_view':
          return {
            ...structuredClone(stats),
            key: {
              ...stats.key,
              generation: doc.generation,
              revision: statisticsRevision,
            },
            phase: 'ready',
            session: 1,
            findings: [{ ...stats.findings[0], cueId: '8', cueIndex: 7 }],
          };
        case 'statistics_cancel':
          return;
        case 'timing_fix_start': {
          starts++;
          if (host.dataset.nextStartError) {
            const error = JSON.parse(host.dataset.nextStartError);
            delete host.dataset.nextStartError;
            throw error;
          }
          if (host.dataset.failStart)
            throw new Error('Injected preview failure');
          if (host.dataset.startDelay)
            await new Promise((resolve) =>
              setTimeout(resolve, Number(host.dataset.startDelay)),
            );
          const input = args.input!;
          activeScope = input.scope ?? null;
          if (episodeTrial) {
            const { options } = input;
            const toggled = input.toggled ?? [];
            let key: keyof typeof episode.views;
            if (incompleteTrial) key = 'incomplete';
            else if (toggled.join(',') === '5') key = 'defaultOff';
            else if (toggled.join(',') === '7') key = 'partial';
            else if (toggled.length === 2) key = 'retick';
            else if (toggled.length === 1) key = 'untick';
            else if (options.sceneEdges === 'nearest') key = 'nearest';
            else if (options.startToleranceMs === 300) key = 'starts';
            else if (options.priority === 'spacing') key = 'spacing';
            else key = 'default';
            const snapshot = structuredClone(
              episode.views[key],
            ) as unknown as Pick<
              TimingFixView,
              | 'rows'
              | 'before'
              | 'after'
              | 'manual'
              | 'changed'
              | 'coverage'
              | 'selectionError'
              | 'selectionConflicts'
              | 'withheldAll'
              | 'withoutTradesToggles'
              | 'trades'
            >;
            if (
              (key === 'untick' && toggled.join(',') !== '2') ||
              (key === 'retick' && toggled.join(',') !== '2,2')
            )
              throw new Error(
                'Missing engine snapshot for checkbox actions ' +
                  toggled.join(','),
              );
            view = {
              ...structuredClone(sample),
              ...snapshot,
              id: starts,
              phase: 'reviewing',
              total: snapshot.rows.length,
              manualCount: snapshot.manual.length,
            };
            return starts;
          }
          if (policyTrial) {
            host.setAttribute('data-timing-request', JSON.stringify(input));
            view = structuredClone(sample);
            if (new URLSearchParams(location.search).has('scroll')) {
              view.rows = Array.from({ length: 160 }, (_, index) => ({
                ...structuredClone(sample.rows[0]!),
                cueIndex: index,
                cueId: String(index + 1),
                cueLabel: String(index + 1),
              }));
            }
            const row = view.rows[0]!;
            row.unit.tradeOff = input.options.priority === 'spacing';
            if (row.unit.tradeOff)
              row.unit.effects = [
                {
                  index: 0,
                  rule: 'cps_high',
                  before: 18,
                  after: 22,
                  limit: 20,
                },
              ];
            if (input.options.startToleranceMs > 0) {
              row.cueLabel = '1→2';
              row.unit.endpoints.push(
                structuredClone(view.rows[1]!.unit.endpoints[0]!),
              );
            }
          }

          view = {
            ...view,
            id: starts,
            phase: 'reviewing',
            rows: view.rows.map((r) => ({
              ...r,
              selected:
                !(r.unit.partial || (input.omitTrades && r.unit.tradeOff)) !==
                ((input.toggled ?? []).filter((index) => index === r.cueIndex)
                  .length %
                  2 ===
                  1),
            })),
          };
          return heldStart ?? starts;
        }
        case 'timing_fix_view': {
          if (
            host.dataset.delayNextView &&
            !args.previewIds &&
            args.section === 'changes'
          ) {
            delete host.dataset.delayNextView;
            host.dataset.reading = 'true';
            await new Promise((resolve) => setTimeout(resolve, 200));
            delete host.dataset.reading;
          }
          const rows = view.rows.filter(
            (r) => !activeScope || activeScope.includes(Number(r.cueId)),
          );
          const manual = view.manual.filter(
            (r) => !activeScope || activeScope.includes(Number(r.cueId)),
          );
          const offset = args.offset ?? 0,
            end = offset + (args.limit ?? 100);
          return {
            ...structuredClone(view),
            total: rows.length,
            changed: rows.filter((r) => r.selected).length,
            manualCount: manual.length,
            withoutTradesToggles: episodeTrial
              ? view.withoutTradesToggles
              : rows
                  .filter((r) => r.selected && r.unit.tradeOff)
                  .map((r) => r.cueIndex),
            trades: rows.filter((r) => r.selected && r.unit.tradeOff).length,
            offset,
            rows:
              args.section === 'manual'
                ? []
                : rows
                    .filter(
                      (r) =>
                        !args.previewIds ||
                        args.previewIds.includes(Number(r.cueId)),
                    )
                    .slice(offset, end),
            manual: args.section === 'manual' ? manual.slice(offset, end) : [],
          };
        }
        case 'timing_fix_cancel':
          cancelled.push(args.id!);
          if (view.id === args.id) view = { ...view, phase: 'cancelled' };
          return;
        case 'timing_fix_accept': {
          if (failAccept) throw new Error('Injected stale scene proposal');
          accepts++;
          before = structuredClone(doc);
          doc = {
            ...doc,
            revision: doc.revision + 1,
            dirty: true,
            canUndo: true,
            cues: doc.cues.map((c) => ({
              ...c,
              startMs:
                view.rows.find((r) => r.selected && r.cueId === String(c.id))
                  ?.ghosts[0]?.after.startMs ?? c.startMs,
              endMs:
                view.rows.find((r) => r.selected && r.cueId === String(c.id))
                  ?.ghosts[0]?.after.endMs ?? c.endMs,
            })),
          };
          return structuredClone(doc);
        }
        case 'quality_read':
          if (activityTrial) {
            host.dataset.reads = String(Number(host.dataset.reads ?? 0) + 1);
            await new Promise((resolve) =>
              setTimeout(resolve, Number(host.dataset.readDelay ?? 0)),
            );
            return emptyQuality(args);
          }
          if (policyTrial || episodeTrial)
            return {
              ...emptyQuality(args),
              nextCue: navDelay === null ? null : '8',
              findingsCount: 3,
              timingFindingsCount: 3,
            };
          throw new Error('Quality must remain disabled in this fixture');
        case 'document_action': {
          const action = args.action!;
          if (action.kind === 'edit') {
            if (navDelay !== null) {
              host.dataset.editAttempts = String(
                Number(host.dataset.editAttempts ?? 0) + 1,
              );
              await new Promise((resolve) =>
                setTimeout(resolve, Number(navDelay)),
              );
              if (host.dataset.rejectEdit)
                throw {
                  kind: 'rejected',
                  message: 'Rejected navigation draft',
                };
            }
            if (failApply) throw new Error('Injected draft failure');
            edits++;
            host.dataset.edits = String(edits);
            doc = {
              ...doc,
              revision: doc.revision + 1,
              dirty: true,
              cues: doc.cues.map((c) =>
                c.id === action.cueId ? { ...c, text: action.text } : c,
              ),
            };
          } else if (action.kind === 'undo') {
            doc = { ...before, revision: doc.revision + 1, canRedo: true };
          } else
            assert(action.kind === 'status', 'Unexpected document operation');
          return structuredClone(doc);
        }
        default:
          throw new Error(`Unexpected IPC ${command}`);
      }
    },
  },
});
const app = mount(App, { target: host });
async function run() {
  try {
    await until(() => host.querySelectorAll('.cue-row').length === 3);
    failApply = true;
    type('Keep this draft');
    await tick();
    await open();
    await until(() => !!host.querySelector('[aria-label="Review panel"]'));
    await until(() => host.textContent!.includes('Injected draft failure'));
    assert(
      starts === 0 && edits === 0,
      'Failed draft commit bypassed preview protection',
    );
    button('Close').click();
    await until(() => !host.querySelector('[aria-label="Review panel"]'));
    const editor = host.querySelector<HTMLTextAreaElement>(
      '[aria-label="Subtitle text"]',
    )!;
    assert(
      editor.value === 'Keep this draft' && edits === 0,
      'Closing review lost failed draft',
    );
    passed(
      'Opening and closing the dock preserves a rejected draft and cannot admit a timing preview over it',
    );
    editor.focus();
    editor.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
    );
    await until(() => editor.value === original.cues[0]!.text);
    failApply = false;
    await open();
    await until(() =>
      [...host.querySelectorAll<HTMLButtonElement>('button')].some(
        (b) => b.textContent?.trim() === 'Apply 2 changes' && !b.disabled,
      ),
    );
    assert(
      JSON.stringify(doc) === JSON.stringify(original),
      'Preview changed document',
    );
    assert(
      !host.querySelector('dialog[open]'),
      'Scene review still occludes video',
    );
    await until(() => host.querySelectorAll('.ghosts .ghost').length === 2);
    passed(
      'Completed scene preview projects both changed endpoints without changing accepted subtitles',
    );
    for (const coverage of ['noMedia', 'unavailable', 'partial'] as const) {
      view = { ...view, coverage };
      await until(() => button('Apply 2 changes').disabled);
      assert(
        ![...host.querySelectorAll('label')].some((label) =>
          label.textContent?.includes('without checking shot'),
        ),
        'Missing scenes could be acknowledged away',
      );
      view = { ...view, coverage: 'checked' };
      await until(() => !button('Apply 2 changes').disabled);
    }
    view = { ...view, coverage: 'reduced' };
    await until(() =>
      host.textContent!.includes('Reduced detection can miss cuts'),
    );
    assert(
      !button('Apply 2 changes').disabled,
      'Completed Reduced scan unnecessarily disabled',
    );
    passed(
      'Missing and partial scenes prevent acceptance; Reduced detection carries an explicit heuristic limitation',
    );
    failAccept = true;
    button('Apply 2 changes').click();
    await until(() =>
      host.textContent!.includes('Injected stale scene proposal'),
    );
    assert(
      accepts === 0 && JSON.stringify(doc) === JSON.stringify(original),
      'Failed acceptance mutated document',
    );
    failAccept = false;
    button('Retry preview').click();
    await until(() =>
      [...host.querySelectorAll<HTMLButtonElement>('button')].some(
        (b) => b.textContent?.trim() === 'Apply 2 changes' && !b.disabled,
      ),
    );
    button('Apply 2 changes').click();
    await until(() => accepts === 1);
    await until(() => !button('Close').disabled);
    assert(
      doc.revision === original.revision + 1 &&
        doc.cues[0]!.startMs === 1130 &&
        doc.cues[1]!.endMs === 5870,
      'Accepted timing mismatch',
    );
    button('Close').click();
    editor.focus();
    button('Undo').click();
    await until(() => doc.cues[0]!.startMs === 1000);
    assert(
      JSON.stringify(doc.cues) === JSON.stringify(original.cues),
      'Undo did not restore all cues',
    );
    passed(
      'Failed acceptance preserves state; explicit acceptance reconciles one revision and one Undo restores all timings',
    );
    view = {
      ...structuredClone(sample),
      total: 0,
      changed: 0,
      rows: [],
      manualCount: 205,
      manual: Array.from({ length: 205 }, (_, i) => ({
        cueId: String(i + 1),
        cueNumber: i + 1,
        detail: {
          index: i,
          lead: 'Would violate minimum duration.',
          facts: [],
          blockingCue: null,
          budgetText: null,
          rules: [],
          budget: null,
        },
      })),
    };
    await open();
    await until(() => host.querySelectorAll('.manual').length === 100);
    assert(
      button('Apply 0 changes').disabled,
      'All-skipped proposal was acceptable',
    );
    button('Next').click();
    await until(() => host.textContent!.includes('101–200 / 205'));
    assert(
      host.querySelectorAll('.manual').length === 100,
      'Unbounded proposal rows',
    );
    passed(
      'Skipped reasons and 100-row proposal paging stay reviewable without enabling an empty acceptance',
    );
    view = { ...view, phase: 'stale' };
    await until(() =>
      host
        .querySelector('[aria-label="Live timing review"]')!
        .textContent!.includes('stale'),
    );
    assert(
      button('Apply 0 changes').disabled,
      'Stale proposal remains acceptable',
    );
    button('Close').click();
    await until(() => !host.querySelector('[aria-label="Review panel"]'));
    passed(
      'Background stale context detection retires an already displayed preview',
    );
    let release!: (id: number) => void;
    heldStart = new Promise((resolve) => (release = resolve));
    const prior = starts;
    await open();
    await until(() => starts > prior);
    button('Close').click();
    await until(() => !host.querySelector('[aria-label="Review panel"]'));
    release(999);
    heldStart = null;
    await until(() => cancelled.includes(999));
    passed(
      'Closing during admission cancels the late returned proposal identity',
    );
    await unmount(app);
    await lifecycle(sample, original, passed);
    heading.textContent = `PASS: all ${output.children.length} scene review checks`;
  } catch (error) {
    heading.textContent = `FAIL: ${String(error)}`;
    await unmount(app);
  }
}
if (policyTrial || episodeTrial) {
  void until(() =>
    navDelay !== null
      ? !!host.querySelector('.cue-row')
      : host.querySelectorAll('.cue-row').length === doc.cues.length,
  ).then(() => chooseCommand(host, 'fixAllTiming'));
} else if (!activityTrial) void run();
