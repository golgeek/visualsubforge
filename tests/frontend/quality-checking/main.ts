import { mount, unmount, tick } from 'svelte';
import App from '../../../src/App.svelte';
import {
  defaultPreferences,
  emptyDocument,
  emptyStatus,
  type DocumentAction,
  type DocumentSnapshot,
  type QualityRequest,
  type QualityView,
} from '../../../src/lib/client';
import { qualityView } from '../ipc-contract-samples';
import { lifecycle } from './lifecycle';

// Production App/client; immutable Rust-generated report, controlled IPC only.
// These tests exercise projection/navigation, not a second quality evaluator.
const host = window.document.querySelector('#test')!;
const heading = window.document.querySelector('#result')!;
const output = window.document.querySelector('#checks')!;
let base: QualityView = structuredClone(qualityView);
let document: DocumentSnapshot = {
  ...emptyDocument,
  loaded: true,
  generation: base.key.generation,
  revision: base.key.revision,
  path: '/fixture/quality.srt',
  cues: [
    {
      id: 1,
      startMs: 1000,
      endMs: 2000,
      text: 'First subtitle, too long for its duration',
    },
    { id: 2, startMs: 2050, endMs: 2450, text: 'Second' },
    { id: 3, startMs: 2300, endMs: 4000, text: 'Third' },
  ],
};
let draft: QualityRequest['draft'] = null;
let edits = 0;
let failApply = false;
let mediaSeeks = 0;
let lastRequest: QualityRequest | null = null;
let heldRead: Promise<void> | null = null;
let holdStarted = false;
const original = JSON.stringify(document);
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(text: string) {
  const row = window.document.createElement('li');
  row.textContent = `PASS: ${text}`;
  output.append(row);
}
function button(text: string) {
  const result = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!result) throw new Error(`Missing button: ${text}`);
  return result;
}
async function until(predicate: () => boolean, message: string) {
  const end = performance.now() + 8000;
  await tick();
  while (!predicate()) {
    if (performance.now() > end) throw new Error(message);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
function input(label: string, value: string) {
  const field = host.querySelector<HTMLInputElement>(
    `[aria-label="${label}"]`,
  )!;
  field.value = value;
  field.dispatchEvent(new Event('input', { bubbles: true }));
}
function selected(id: number) {
  return (
    host.querySelectorAll('.cue-row')[id - 1]?.getAttribute('aria-pressed') ===
    'true'
  );
}
function filter(label: string, value: string) {
  const select = host.querySelector<HTMLSelectElement>(
    `[aria-label="${label}"]`,
  )!;
  select.value = value;
  select.dispatchEvent(new Event('change', { bubbles: true }));
}
function read(request: QualityRequest): QualityView {
  lastRequest = request;
  if (request.draft) draft = request.draft;
  const view = structuredClone(base);
  view.key = {
    ...view.key,
    generation: document.generation,
    revision: document.revision,
  };
  view.findings = base.findings.filter(
    (f) =>
      (!request.rule || request.rule === f.rule) &&
      (!request.severity || request.severity === f.severity),
  );
  const ids = new Set(
    view.findings.flatMap((f) => [
      f.cueId,
      ...(f.relatedCueId ? [f.relatedCueId] : []),
    ]),
  );
  const cues = document.cues.filter((c) => ids.has(String(c.id)));
  const index = document.cues.findIndex((c) => c.id === request.selected);
  view.filteredCount = view.findings.length;
  view.filteredCues = cues.length;
  view.findingsCount = base.findings.length;
  view.offset = request.offset;
  view.findings = view.findings.slice(
    request.offset,
    request.offset + request.limit,
  );
  view.nextCue =
    String(
      (cues.find((c) => document.cues.indexOf(c) > index) ?? cues[0])?.id ?? '',
    ) || null;
  view.previousCue =
    String(
      (
        cues
          .slice()
          .reverse()
          .find((c) => document.cues.indexOf(c) < index) ?? cues.at(-1)
      )?.id ?? '',
    ) || null;
  view.selected =
    base.metrics.find((m) => m.data.cueId === String(request.selected)) ?? null;
  view.metrics = base.metrics.slice(
    request.metricsOffset,
    request.metricsOffset + request.metricsLimit,
  );
  view.draft =
    draft?.kind === 'update'
      ? {
          ...structuredClone(base.metrics[0]!),
          data: {
            ...structuredClone(base.metrics[0]!.data),
            cueId: String(draft.value.cueId),
            characters: 123,
          },
        }
      : null;
  return view;
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
      args: { action?: DocumentAction; request?: QualityRequest },
    ) => {
      switch (command) {
        case 'plugin:event|listen':
          return 1;
        case 'plugin:event|unlisten':
        case 'update_menu':
        case 'quality_cancel':
          return;
        case 'preferences_get':
          return {
            ...structuredClone(defaultPreferences),
            showQualityMetrics: true,
          };
        case 'recovery_status':
          return { entries: [], error: null };
        case 'media_action':
          if ((args.action?.kind as string) === 'seek') mediaSeeks++;
          return structuredClone(emptyStatus);
        case 'quality_read': {
          const view = read(args.request!);
          if (heldRead) {
            holdStarted = true;
            await heldRead;
          }
          return view;
        }
        case 'document_action': {
          const action = args.action!;
          if (action.kind === 'edit') {
            if (failApply)
              throw new Error('Injected quality navigation apply failure');
            edits++;
            document = {
              ...document,
              revision: document.revision + 1,
              dirty: true,
              canUndo: true,
              cues: document.cues.map((c) =>
                c.id === action.cueId
                  ? {
                      id: c.id,
                      startMs: action.startMs,
                      endMs: action.endMs,
                      text: action.text,
                    }
                  : c,
              ),
            };
          } else
            assert(
              action.kind === 'status',
              `Unexpected document action: ${action.kind}`,
            );
          return structuredClone(document);
        }
        default:
          throw new Error(`Unexpected quality IPC: ${command}`);
      }
    },
  },
});

const app = mount(App, { target: host });
async function run() {
  try {
    await until(
      () => !!host.querySelector('[aria-label="Accepted cue metrics"]'),
      'Accepted metrics unavailable',
    );
    assert(
      JSON.stringify(document) === original && edits === 0,
      'Checking changed subtitles',
    );
    passed(
      'Accepted metrics appear without media and checking leaves document/history unchanged',
    );
    [...host.querySelectorAll('button')]
      .find((b) => b.textContent?.trim().startsWith('Problems'))!
      .click();
    await until(
      () => !!host.querySelector('[aria-label="Subtitle Problems"] .finding'),
      'Problems did not appear',
    );
    assert(
      host.textContent!.includes(`${base.findingsCount} findings`),
      'Finding count missing',
    );
    passed(
      'Problems shows measured/expected values and separate finding/affected-cue counts',
    );
    filter('Problem rule', 'gap');
    await until(
      () => lastRequest?.rule === 'gap' && !button('Next cue · F4').disabled,
      'Gap filter did not settle',
    );
    assert(
      host.querySelectorAll('.finding').length === 1 &&
        host.textContent!.includes('1 matching findings · 2 cues'),
      'Gap pair did not count both distinct cues',
    );
    button('Next cue · F4').click();
    await until(() => selected(2), 'Next problem did not select cue 2');
    assert(
      edits === 0 && mediaSeeks === 0,
      'Navigation edited subtitles or sought absent media',
    );
    passed(
      'Filtered navigation visits distinct related cues and works without media',
    );
    const workspace = host as HTMLElement;
    const originalWidth = workspace.style.width;
    try {
      // Exercise natural toolbar wrapping, including the suspended-following
      // controls, without changing font metrics or substituting layout CSS.
      for (const width of [1100, 820, 1100]) {
        workspace.style.width = `${width}px`;
        await until(() => {
          const rect = (selector: string) =>
            host.querySelector(selector)!.getBoundingClientRect();
          const list = rect('.cue-list');
          const table = rect('.subtitle-panel');
          const editor = rect('.editor');
          const text = rect('[aria-label="Subtitle text"]');
          return (
            list.height >= 64 &&
            text.height >= 34 &&
            list.top >= rect('.cue-followbar').bottom &&
            list.bottom <= table.bottom &&
            table.bottom <= editor.top &&
            text.bottom <= rect('.editorfooter').top &&
            rect('.editorfooter').bottom <= editor.bottom
          );
        }, `Wrapped controls squeeze or overlap editing panes at ${width}px`);
      }
    } finally {
      workspace.style.width = originalWidth;
    }
    passed(
      'Wrapped controls preserve two cue rows and a usable editor without overlap',
    );
    input('Subtitle text', 'Unapplied quality draft');
    await until(
      () =>
        !!host.querySelector('[aria-label="Draft · not applied cue metrics"]'),
      'Labeled draft metrics absent',
    );
    assert(
      edits === 0 && document.cues[1]!.text === 'Second',
      'Draft metrics committed text',
    );
    passed(
      'Draft metrics are labeled and remain separate from accepted report/state',
    );
    button('Next cue · F4').click();
    await until(
      () =>
        !!host.querySelector(
          '[aria-label="Resolve cue draft before navigation"]',
        ),
      'Draft navigation protection absent',
    );
    button('Cancel navigation').click();
    assert(
      selected(2) && edits === 0,
      'Cancel navigation lost draft or selection',
    );
    passed('Cancel navigation preserves the current draft and selection');
    button('Next cue · F4').click();
    await tick();
    failApply = true;
    button('Apply draft and navigate').click();
    await until(
      () =>
        host.textContent!.includes('Injected quality navigation apply failure'),
      'Failed apply error absent',
    );
    assert(
      selected(2) &&
        edits === 0 &&
        !!host.querySelector(
          '[aria-label="Resolve cue draft before navigation"]',
        ),
      'Failed apply navigated or discarded the draft',
    );
    passed(
      'Failed draft Apply prevents navigation and preserves the pending choice',
    );
    failApply = false;
    button('Apply draft and navigate').click();
    await until(() => selected(1), 'Explicit Apply did not navigate');
    assert(
      edits === 1 && document.cues[1]!.text === 'Unapplied quality draft',
      'Explicit Apply did not commit once',
    );
    passed('Explicit Apply commits once before protected navigation');
    await until(
      () => !button('Next cue · F4').disabled,
      'Recheck did not finish after edit',
    );
    input('Subtitle text', 'Discard this draft');
    await tick();
    await until(
      () => !button('Next cue · F4').disabled,
      'Draft metric refresh did not finish',
    );
    button('Next cue · F4').click();
    await tick();
    button('Discard draft and navigate').click();
    await until(() => selected(2), 'Discard did not navigate');
    assert(
      edits === 1 && document.cues[0]!.text.startsWith('First subtitle'),
      'Discard changed accepted text',
    );
    passed('Explicit Discard navigates without an edit');
    await until(
      () => !button('Previous cue · ⇧F4').disabled,
      'Navigation still updating',
    );
    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'F4',
        shiftKey: true,
        bubbles: true,
      }),
    );
    await until(() => selected(1), 'Shift+F4 did not navigate');
    passed(
      'Keyboard problem navigation uses the same protected selection path',
    );
    input('Cue start', 'not a timestamp');
    await until(
      () => host.textContent!.includes('Draft metrics unavailable'),
      'Invalid draft time was not labeled',
    );
    assert(edits === 1, 'Invalid draft changed accepted state');
    button('Revert draft').click();
    passed(
      'Invalid draft timings leave accepted metrics and editing state available',
    );
    filter('Problem rule', '');
    base = {
      ...base,
      profile: {
        ...base.profile,
        id: 'fixture-partial',
        name: 'Partial fixture',
        limitations: ['Unsupported scene rules in this fixture'],
      },
    };
    base.key.epoch++;
    await until(
      () => host.textContent!.includes('Partial coverage'),
      'Partial coverage label missing',
    );
    filter('Problem severity', 'advisory');
    await until(
      () => lastRequest?.severity === 'advisory',
      'Severity filter not sent',
    );
    passed('Severity filters and partial-coverage status remain visible');
    filter('Problem severity', '');
    base = {
      ...base,
      findings: Array.from({ length: 205 }, (_, i) => ({
        ...base.findings[0]!,
        measured: i,
      })),
    };
    button('Check again').click();
    await until(
      () => host.querySelectorAll('.finding').length === 100,
      'Findings were not bounded to a page',
    );
    button('Next page').click();
    await until(
      () =>
        lastRequest?.offset === 100 &&
        host.textContent!.includes('101–200 of 205'),
      'Finding pagination failed',
    );
    assert(
      host.querySelectorAll('.finding').length === 100,
      'Page exceeded its row bound',
    );
    passed(
      'Large result lists page 100 findings without rendering the entire report',
    );
    let release!: () => void;
    heldRead = new Promise<void>((resolve) => {
      release = resolve;
    });
    button('Check again').click();
    await until(() => holdStarted, 'Controlled quality read did not start');
    input('Subtitle text', 'Typing during a check');
    await tick();
    assert(
      !host.querySelector<HTMLTextAreaElement>('[aria-label="Subtitle text"]')!
        .readOnly && edits === 1,
      'Checking blocked editing or committed a draft',
    );
    const cancelledAt = performance.now();
    button('Pause checking').click();
    await until(
      () => host.textContent!.includes('Checking paused'),
      'Pause was not visible',
    );
    assert(
      performance.now() - cancelledAt < 1000,
      'Pause visibility exceeded one second',
    );
    heldRead = null;
    release();
    button('Revert draft').click();
    passed(
      'Editing and visible cancellation stay responsive while a check response is held',
    );
    button('Check again').click();
    await until(
      () => !host.textContent!.includes('Checking paused'),
      'Restart remained paused',
    );
    passed('Checking can be paused and restarted without disabling the editor');
    await unmount(app);
    await lifecycle(structuredClone(qualityView), passed);
    heading.textContent = `PASS: all ${output.children.length} quality checking checks`;
  } catch (e) {
    heading.textContent = `FAIL: ${String(e)}`;
    await unmount(app);
  }
}
void run();
