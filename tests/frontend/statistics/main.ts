import { mount, unmount, tick } from 'svelte';
import App from '../../../src/App.svelte';
import {
  emptyDocument,
  emptyStatus,
  defaultPreferences,
  type DocumentSnapshot,
  type DocumentAction,
  type StatisticsView,
  type StatisticsSelection,
} from '../../../src/lib/client';
import {
  statisticsView as sample,
  qualityView as qualitySample,
} from '../ipc-contract-samples';
import { lifecycle } from './lifecycle';
const host = document.querySelector('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
const original: DocumentSnapshot = {
  ...emptyDocument,
  loaded: true,
  generation: sample.key.generation,
  revision: sample.key.revision,
  path: '/fixture.srt',
  cues: [
    { id: 1, startMs: 0, endMs: 2500, text: '{\\i1}A😀' },
    { id: 2, startMs: 2000, endMs: 2400, text: '0' },
    { id: 3, startMs: 4000, endMs: 6000, text: '<i>Other text</i>' },
  ],
};
let doc = structuredClone(original);
let view: StatisticsView = structuredClone(sample);
let starts = 0,
  edits = 0;
let failApply = false;
let heldStart: Promise<number> | null = null;
const cancelled: number[] = [];
const selections: StatisticsSelection[] = [];
function assert(v: unknown, m: string) {
  if (!v) throw new Error(m);
}
function passed(m: string) {
  const li = document.createElement('li');
  li.textContent = `PASS: ${m}`;
  output.append(li);
}
function button(text: string) {
  const b = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!b) throw new Error(`Missing ${text}`);
  return b;
}
async function until(check: () => boolean) {
  await tick();
  const end = performance.now() + 7000;
  while (!check()) {
    assert(performance.now() < end, 'Timed out: ' + host.textContent);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
function input(label: string, text: string) {
  const e = host.querySelector<HTMLInputElement | HTMLTextAreaElement>(
    `[aria-label="${label}"]`,
  )!;
  e.value = text;
  e.dispatchEvent(new Event('input', { bubbles: true }));
}
function select(label: string, value: string) {
  const e = host.querySelector<HTMLSelectElement>(`[aria-label="${label}"]`)!;
  e.value = value;
  e.dispatchEvent(new Event('change', { bubbles: true }));
}
async function open() {
  await until(() => !button('Statistics…').disabled);
  button('Statistics…').click();
  await until(() => !!host.querySelector('dialog[open]'));
}
async function report() {
  await until(() => !!host.querySelector('[aria-label="Metric summaries"]'));
}
async function close() {
  button('Close statistics').click();
  await until(() => !host.querySelector('dialog[open]'));
  await until(() => !button('Statistics…').disabled);
}
async function navigate(cue: number) {
  const b = host.querySelector<HTMLButtonElement>(
    `button[title="Go to cue ${cue}"]`,
  )!;
  assert(b, 'Missing extremum navigation');
  b.click();
  await until(() => !host.querySelector('dialog[open]'));
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
        id?: number;
        offset?: number;
        limit?: number;
        selection?: StatisticsSelection;
      },
    ) => {
      switch (command) {
        case 'plugin:event|listen':
          return 1;
        case 'plugin:event|unlisten':
        case 'update_menu':
          return;
        case 'preferences_get':
          return structuredClone(defaultPreferences);
        case 'recovery_status':
          return { entries: [], error: null };
        case 'media_action':
          return structuredClone(emptyStatus);
        case 'statistics_start':
          starts++;
          selections.push(structuredClone(args.selection!));
          view = {
            ...view,
            id: starts,
            session: 0,
            key: {
              ...view.key,
              generation: doc.generation,
              revision: doc.revision,
            },
          };
          if (args.selection?.excluded.includes(99))
            throw new Error(
              'Excluded cue numbers must be within the selected scope',
            );
          return heldStart ?? starts;
        case 'statistics_view':
          return {
            ...structuredClone(view),
            offset: args.offset ?? 0,
            findings: view.findings.slice(
              args.offset ?? 0,
              (args.offset ?? 0) + (args.limit ?? 100),
            ),
          };
        case 'statistics_cancel':
          cancelled.push(args.id!);
          return;
        case 'quality_read':
          return {
            ...structuredClone(qualitySample),
            key: {
              ...qualitySample.key,
              generation: doc.generation,
              revision: doc.revision,
            },
          };
        case 'quality_cancel':
          return;
        case 'document_action': {
          const action = args.action!;
          if (action.kind === 'edit') {
            if (failApply) throw new Error('Injected draft failure');
            edits++;
            doc = {
              ...doc,
              revision: doc.revision + 1,
              dirty: true,
              canUndo: true,
              cues: doc.cues.map((c) =>
                c.id === action.cueId
                  ? {
                      id: c.id,
                      text: action.text,
                      startMs: action.startMs,
                      endMs: action.endMs,
                    }
                  : c,
              ),
            };
          } else if (action.kind !== 'status')
            throw new Error(`Unexpected document mutation ${action.kind}`);
          return structuredClone(doc);
        }
        default:
          throw new Error(`Unexpected IPC ${command}`);
      }
    },
  },
});
let app = mount(App, { target: host });
try {
  await until(() => !button('Statistics…').disabled);
  input('Subtitle text', 'Keep my statistics draft');
  const before = JSON.stringify(doc);
  await open();
  await report();
  assert(
    host.textContent
      ?.replace(/\s+/g, ' ')
      .includes('not included in these statistics'),
    'Draft inclusion not labeled',
  );
  assert(
    host.querySelectorAll('[aria-label="Reading-speed distribution"] tbody tr')
      .length === 9,
    'All categories required',
  );
  assert(
    JSON.stringify(doc) === before && edits === 0,
    'Opening stats mutated accepted state',
  );
  await close();
  assert(
    (host.querySelector('[aria-label="Subtitle text"]') as HTMLTextAreaElement)
      .value === 'Keep my statistics draft',
    'Close changed draft',
  );
  passed(
    'Read-only accepted statistics preserve drafts, tags, history and all nine category rows',
  );
  await open();
  await report();
  await navigate(2);
  await until(
    () =>
      !![...host.querySelectorAll('button')].find(
        (b) => b.textContent === 'Cancel navigation',
      ),
  );
  button('Cancel navigation').click();
  assert(
    JSON.stringify(doc) === before && edits === 0,
    'Cancel changed accepted document',
  );
  assert(
    (host.querySelector('[aria-label="Subtitle text"]') as HTMLTextAreaElement)
      .value === 'Keep my statistics draft',
    'Cancel lost draft',
  );
  passed('Extremum navigation uses the existing draft Cancel guard');
  await open();
  await report();
  await navigate(2);
  await until(
    () =>
      !![...host.querySelectorAll('button')].find(
        (b) => b.textContent === 'Discard draft and navigate',
      ),
  );
  button('Discard draft and navigate').click();
  await until(
    () =>
      (
        host.querySelector(
          '[aria-label="Subtitle text"]',
        ) as HTMLTextAreaElement
      ).value === '0',
  );
  assert(
    JSON.stringify(doc) === before,
    'Discard navigation wrote accepted state',
  );
  passed('Discard navigation changes selection without accepting a draft');
  input('Subtitle text', 'Explicitly applied draft');
  await open();
  await report();
  await navigate(1);
  await until(
    () =>
      !![...host.querySelectorAll('button')].find(
        (b) => b.textContent === 'Apply draft and navigate',
      ),
  );
  failApply = true;
  button('Apply draft and navigate').click();
  await until(
    () => host.textContent?.includes('Injected draft failure') ?? false,
  );
  assert(JSON.stringify(doc) === before, 'Failed Apply changed document');
  failApply = false;
  button('Apply draft and navigate').click();
  await until(() => doc.revision === original.revision + 1);
  await until(
    () =>
      (
        host.querySelector(
          '[aria-label="Subtitle text"]',
        ) as HTMLTextAreaElement
      ).value === original.cues[0]!.text,
  );
  assert(
    edits === 1 && doc.cues[1]!.text === 'Explicitly applied draft',
    'Explicit navigation Apply did not commit once',
  );
  passed(
    'Failed Apply preserves draft and retry commits only the explicit navigation choice',
  );
  await open();
  await report();
  select('Statistics scope', 'range');
  await tick();
  input('First statistics cue', '1');
  input('Last statistics cue', '3');
  input('Excluded statistics cues', '2, 2');
  await tick();
  assert(
    !host.querySelector('[aria-label="Metric summaries"]'),
    'Changed scope kept results usable',
  );
  button('Calculate statistics').click();
  await report();
  assert(
    JSON.stringify(selections.at(-1)) ===
      JSON.stringify({ range: { first: 1, last: 3 }, excluded: [2, 2] }),
    'Range/exclusions not sent intact to Rust',
  );
  input('Excluded statistics cues', 'bad');
  await tick();
  assert(
    button('Calculate statistics').disabled &&
      !host.querySelector('[aria-label="Metric summaries"]'),
    'Invalid form retains report',
  );
  input('Excluded statistics cues', '99');
  await until(() => !button('Calculate statistics').disabled);
  button('Calculate statistics').click();
  await until(
    () => host.textContent?.includes('within the selected scope') ?? false,
  );
  assert(
    !host.querySelector('[aria-label="Metric summaries"]'),
    'Failed scope retained report',
  );
  await close();
  passed(
    'Scope changes retire results and invalid or failed exclusions cannot publish old data',
  );
  view = structuredClone(sample);
  view.findings = Array.from({ length: 205 }, (_, i) => ({
    ...sample.findings[0]!,
    cueIndex: i,
    cueId: String(i + 1),
  }));
  view.summary!.findingsCount = 205;
  await open();
  await report();
  assert(
    host.querySelectorAll('.finding').length === 100,
    'First page not bounded',
  );
  button('Next findings').click();
  await until(() => host.textContent?.includes('101–200 of 205') ?? false);
  assert(
    host.querySelectorAll('.finding').length === 100,
    'Second page not bounded',
  );
  await close();
  passed(
    'Outliers page 100 findings at a time without changing accepted state',
  );
  view = {
    ...structuredClone(sample),
    phase: 'stale',
    summary: null,
    findings: [],
  };
  await open();
  await until(
    () => host.textContent?.includes('Document or profile changed') ?? false,
  );
  assert(
    !host.querySelector('[aria-label="Metric summaries"]'),
    'Stale report visible',
  );
  await close();
  passed('Rust stale results hide metrics and outlier navigation');
  view = structuredClone(sample);
  let release!: (id: number) => void;
  heldStart = new Promise((r) => {
    release = r;
  });
  await open();
  const pendingId = starts;
  await close();
  release(pendingId);
  await until(() => cancelled.includes(pendingId));
  heldStart = null;
  passed(
    'Closing during admission cancels the eventual job and leaves the workspace usable',
  );
  await unmount(app);
  doc = {
    ...structuredClone(original),
    cues: [],
    generation: original.generation + 1,
  };
  view = structuredClone(sample);
  const s = view.summary!;
  s.totalCues = 0;
  s.scopeCues = 0;
  s.includedCues = 0;
  s.findingsCount = 0;
  s.affectedCues = 0;
  s.rules = [];
  s.ratings = s.ratings.map((r) => ({ ...r, count: 0, percentage: null }));
  for (const key of [
    'durationMs',
    'cps',
    'rs',
    'lineCount',
    'longestLine',
    'gapsMs',
  ] as const)
    s[key] = { count: 0, minimum: null, maximum: null, mean: null };
  view.findings = [];
  app = mount(App, { target: host });
  await open();
  await report();
  assert(
    host.textContent?.includes('Denominator: 0 cues'),
    'Empty denominator not explained',
  );
  assert(
    host.textContent?.includes('unavailable'),
    'Empty means not unavailable',
  );
  await close();
  passed(
    'Empty loaded documents have a usable Statistics view with null summaries',
  );
  await lifecycle(sample, original, passed);
  heading.textContent = `PASS: all ${output.children.length} statistics workflow checks`;
} catch (error) {
  heading.textContent = 'FAILED';
  const li = document.createElement('li');
  li.textContent = `FAIL: ${String(error)}`;
  output.append(li);
  console.error(error);
} finally {
  await unmount(app);
}
