import { mount, unmount, tick } from 'svelte';
import App from '../../../src/App.svelte';
import {
  emptyDocument,
  emptyStatus,
  defaultPreferences,
  type DocumentAction,
  type BatchTimingRequest,
  type BatchTimingView,
} from '../../../src/lib/client';
import { batchTimingView as sample } from '../ipc-contract-samples';
const host = document.querySelector('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
let doc = {
  ...emptyDocument,
  loaded: true,
  generation: 1,
  revision: 0,
  path: '/fixture.srt',
  cues: Array.from({ length: 6 }, (_, i) => ({
    id: i + 1,
    startMs: 1000 + i * 2000,
    endMs: 2000 + i * 2000,
    text: `Cue ${i + 1} {\\k20}é😀`,
  })),
};
let before = structuredClone(doc),
  after = structuredClone(doc);
let view: BatchTimingView = structuredClone(sample);
let request: BatchTimingRequest | null = null;
let failEdit = false,
  failAccept = false;
let starts = 0,
  accepts = 0;
let releaseStart: ((id: number) => void) | null = null;
let holdStart = false;
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
  const b = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!b) throw new Error(`Missing ${text}`);
  return b;
}
function field(label: string) {
  const f = host.querySelector<
    HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement
  >(`[aria-label="${label}"]`);
  if (!f) throw new Error(`Missing ${label}`);
  return f;
}
function input(label: string, value: string) {
  const f = field(label);
  f.value = value;
  f.dispatchEvent(
    new Event(f instanceof HTMLSelectElement ? 'change' : 'input', {
      bubbles: true,
    }),
  );
}
function row(id: number) {
  const r = host.querySelector<HTMLButtonElement>(`[data-cue-id="${id}"]`);
  if (!r) throw new Error(`Missing cue ${id}`);
  return r;
}
const selected = () =>
  [
    ...host.querySelectorAll<HTMLButtonElement>(
      '.cue-row[aria-pressed="true"]',
    ),
  ].map((r) => Number(r.dataset.cueId));
async function until(check: () => boolean) {
  await tick();
  const end = performance.now() + 6000;
  while (!check()) {
    assert(performance.now() < end, 'Timed out: ' + host.textContent);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
async function clickCue(id: number, modifiers: MouseEventInit = {}) {
  await until(() => row(id).getAttribute('aria-disabled') === 'false');
  row(id).dispatchEvent(
    new MouseEvent('click', { bubbles: true, ...modifiers }),
  );
  await until(() => row(id).getAttribute('aria-disabled') === 'false');
}
async function open() {
  await until(() => !button('Shift / align cues…').disabled);
  button('Shift / align cues…').click();
  await until(() => !!host.querySelector('dialog[open]'));
}
async function preview() {
  const label = () =>
    host.querySelector('dialog[open]')?.textContent?.includes('Preview again')
      ? 'Preview again'
      : 'Create batch preview';
  await until(() => !button(label()).disabled);
  button(label()).click();
  await until(() => !button('Accept batch timings').disabled);
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
        request?: BatchTimingRequest;
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
          return {
            ...emptyStatus,
            loaded: true,
            paused: true,
            positionSeconds: 5.015,
          };
        case 'quality_read':
          throw new Error('Quality disabled in this fixture');
        case 'choose_file':
          return null;
        case 'batch_timing_start': {
          request = structuredClone(args.request!);
          starts++;
          const scope = request.scope;
          const cues = doc.cues.filter(
            (c) =>
              scope.kind === 'all' ||
              (scope.kind === 'selected' && scope.cueIds.includes(c.id)) ||
              (scope.kind === 'following' && c.id >= scope.cueId) ||
              (scope.kind === 'atOrAfter' && c.startMs >= scope.positionMs),
          );
          const delta =
            request.change.kind === 'shift'
              ? request.change.amountMs * (request.change.earlier ? -1 : 1)
              : request.change.playheadMs -
                Math.min(...cues.map((c) => c.startMs));
          view = {
            ...sample,
            id: starts,
            key: {
              ...sample.key,
              generation: doc.generation,
              revision: doc.revision,
            },
            total: cues.length,
            changed: cues.length,
            offsetMs: String(delta),
            alignedMs:
              request.change.kind === 'align'
                ? request.change.playheadMs
                : null,
            rows: cues.map((c) => ({
              cueId: String(c.id),
              cueIndex: c.id - 1,
              oldStartMs: c.startMs,
              oldEndMs: c.endMs,
              newStartMs: c.startMs + delta,
              newEndMs: c.endMs + delta,
            })),
          };
          return holdStart
            ? new Promise<number>((r) => {
                releaseStart = r;
              })
            : starts;
        }
        case 'batch_timing_view':
          return {
            ...structuredClone(view),
            offset: args.offset ?? 0,
            rows: view.rows.slice(
              args.offset ?? 0,
              (args.offset ?? 0) + (args.limit ?? 100),
            ),
          };
        case 'batch_timing_cancel':
          cancelled.push(args.id!);
          return;
        case 'batch_timing_accept': {
          if (failAccept) throw new Error('Subtitles changed; preview again');
          accepts++;
          before = structuredClone(doc);
          doc = {
            ...doc,
            revision: doc.revision + 1,
            dirty: true,
            canUndo: true,
            canRedo: false,
            cues: doc.cues.map((c) => {
              const r = view.rows.find((r) => r.cueId === String(c.id));
              return r ? { ...c, startMs: r.newStartMs, endMs: r.newEndMs } : c;
            }),
          };
          after = structuredClone(doc);
          return structuredClone(doc);
        }
        case 'document_action': {
          const a = args.action!;
          if (a.kind === 'edit') {
            if (failEdit) throw new Error('Invalid cue draft');
            doc = {
              ...doc,
              revision: doc.revision + 1,
              cues: doc.cues.map((c) =>
                c.id === a.cueId
                  ? { ...c, startMs: a.startMs, endMs: a.endMs, text: a.text }
                  : c,
              ),
            };
          } else if (a.kind === 'undo')
            doc = { ...before, revision: doc.revision + 1, canRedo: true };
          else if (a.kind === 'redo')
            doc = { ...after, revision: doc.revision + 1 };
          else assert(a.kind === 'status', `Unexpected action ${a.kind}`);
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
    await until(() => host.querySelectorAll('.cue-row').length === 6);
    await clickCue(2);
    await clickCue(4, { shiftKey: true });
    assert(
      JSON.stringify(selected()) === '[2,3,4]',
      'Shift click did not select contiguous range',
    );
    await clickCue(6, { ctrlKey: true });
    assert(
      JSON.stringify(selected()) === '[2,3,4,6]',
      'Toggle did not add disjoint cue',
    );
    await clickCue(3, { metaKey: true });
    assert(
      JSON.stringify(selected()) === '[2,4,6]',
      'Meta toggle did not remove cue',
    );
    passed('Mouse ranges and Ctrl/Cmd disjoint selection share stable cue IDs');

    await clickCue(2);
    row(2).dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'ArrowDown',
        shiftKey: true,
        bubbles: true,
      }),
    );
    await until(() => JSON.stringify(selected()) === '[2,3]');
    await until(() => document.activeElement === row(3));
    row(3).dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'ArrowDown',
        ctrlKey: true,
        bubbles: true,
      }),
    );
    await until(() => document.activeElement === row(4));
    assert(
      JSON.stringify(selected()) === '[2,3]',
      'Ctrl arrow changed membership',
    );
    row(4).dispatchEvent(
      new KeyboardEvent('keydown', {
        key: ' ',
        code: 'Space',
        ctrlKey: true,
        bubbles: true,
      }),
    );
    await until(() => JSON.stringify(selected()) === '[2,3,4]');
    row(4).dispatchEvent(
      new KeyboardEvent('keydown', { key: 'a', metaKey: true, bubbles: true }),
    );
    await until(() => selected().length === 6);
    passed('Keyboard range, focus-only movement, toggle and Select All work');

    input('Subtitle text', 'unapplied');
    failEdit = true;
    await clickCue(1);
    assert(
      selected().length === 6 && field('Subtitle text').value === 'unapplied',
      'Failed draft application changed selection',
    );
    await open();
    assert(
      button('Create batch preview').disabled,
      'Draft did not protect preview',
    );
    button('Discard cue draft').click();
    failEdit = false;
    await preview();
    assert(
      request?.scope.kind === 'selected' && request.scope.cueIds.length === 6,
      'Selection not captured in proposal',
    );
    assert(
      host.querySelectorAll('dialog[open] tbody tr').length === 6 && !doc.dirty,
      'Preview changed document or missing review',
    );
    input('Batch offset milliseconds', '200');
    await tick();
    assert(
      button('Accept batch timings').disabled,
      'Changed offset retained acceptance',
    );
    await until(
      () =>
        host
          .querySelector('dialog[open]')
          ?.textContent?.includes('preview again') ?? false,
    );
    await preview();
    failAccept = true;
    button('Accept batch timings').click();
    await until(
      () =>
        host
          .querySelector('dialog[open] [role="alert"]')
          ?.textContent?.includes('Subtitles changed') ?? false,
    );
    assert(
      accepts === 0 && selected().length === 6,
      'Rejected acceptance mutated state',
    );
    failAccept = false;
    button('Accept batch timings').click();
    await until(() => !host.querySelector('dialog[open]'));
    assert(
      doc.cues[0]!.startMs === 1200 && accepts === 1 && selected().length === 6,
      'Batch not reconciled with selection',
    );
    await until(() => !button('Undo edit').disabled);
    button('Undo edit').click();
    await until(() => doc.cues[0]!.startMs === 1000);
    assert(selected().length === 6, 'Undo discarded selection');
    await until(() => !button('Redo edit').disabled);
    button('Redo edit').click();
    await until(() => doc.cues[0]!.startMs === 1200);
    passed(
      'Draft/error guards, immutable review, one-step history and selection reconciliation',
    );

    await clickCue(2);
    await clickCue(4, { ctrlKey: true });
    await open();
    input('Batch scope', 'following');
    await preview();
    assert(
      request?.scope.kind === 'following' && request.scope.cueId === 4,
      'Following scope not based on current cue',
    );
    input('Batch operation', 'align');
    await tick();
    assert(
      button('Accept batch timings').disabled,
      'Changed operation retained acceptance',
    );
    await until(() => cancelled.includes(starts));
    await preview();
    assert(
      request?.change.kind === 'align' &&
        request.change.playheadMs === 5015 &&
        request.scope.kind === 'selected' &&
        request.scope.cueIds.length === 2,
      'Alignment lost captured playhead or disjoint selection',
    );
    button('Close batch review').click();
    await until(() => !host.querySelector('dialog[open]'));
    passed(
      'Following rows and playhead alignment capture explicit scope and displayed target',
    );

    await open();
    holdStart = true;
    button('Create batch preview').click();
    await until(() => releaseStart !== null);
    button('Close batch review').click();
    await until(() => !host.querySelector('dialog[open]'));
    releaseStart!(starts);
    await until(() => cancelled.includes(starts));
    assert(accepts === 1, 'Closed dialog accepted a late start');
    passed('Closing during delayed admission cancels the late proposal');
    heading.textContent = `PASS: all ${output.children.length} batch timing checks`;
  } catch (e) {
    heading.textContent = `FAIL: starts=${starts} accepts=${accepts} ${e instanceof Error ? `${e.message} ${e.stack}` : String(e)}`;
  } finally {
    await unmount(app);
  }
}
void run();
