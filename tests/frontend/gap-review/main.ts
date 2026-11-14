import { chooseCommand } from '../command-discovery';
import { mount, unmount, tick } from 'svelte';
import App from '../../../src/App.svelte';
import {
  emptyDocument,
  emptyStatus,
  defaultPreferences,
  type DocumentSnapshot,
  type DocumentAction,
  type GapView,
  type ReviewSelection,
} from '../../../src/lib/client';
import { gapView as sample } from '../ipc-contract-samples';
import { lifecycle } from './lifecycle';
const host = document.querySelector('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
let doc: DocumentSnapshot = {
  ...emptyDocument,
  loaded: true,
  generation: sample.key.generation,
  revision: sample.key.revision,
  path: '/fixture.srt',
  cues: [
    { id: 1, startMs: 1000, endMs: 2000, text: '{\\i1}First' },
    { id: 2, startMs: 2050, endMs: 3050, text: 'Second' },
    { id: 3, startMs: 3100, endMs: 4100, text: 'Third' },
  ],
};
const original = structuredClone(doc);
let view: GapView = structuredClone(sample);
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
  await until(() => !button('Commands').disabled);
  await chooseCommand(host, 'fixGaps');
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
        acknowledgeNoMedia?: boolean;
        selection?: ReviewSelection;
        previewIds?: number[];
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
          return structuredClone(defaultPreferences);
        case 'recovery_status':
          return { entries: [], error: null };
        case 'media_action':
          return structuredClone(emptyStatus);
        case 'gap_start':
          starts++;
          view = {
            ...view,
            id: starts,
            phase: 'reviewing',
            rows: view.rows.map((row) => ({
              ...row,
              selected:
                row.newEndMs !== null &&
                !args.selection?.excluded.includes(Number(row.cueId)),
            })),
            key: {
              ...view.key,
              generation: doc.generation,
              revision: doc.revision,
            },
          };
          view.changed = view.rows.filter((row) => row.selected).length;
          return heldStart ?? starts;
        case 'gap_view':
          return {
            ...structuredClone(view),
            offset: args.offset ?? 0,
            rows: view.rows
              .filter(
                (row) =>
                  !args.previewIds ||
                  args.previewIds.includes(Number(row.cueId)),
              )
              .slice(
                args.offset ?? 0,
                (args.offset ?? 0) + (args.limit ?? 100),
              ),
          };
        case 'gap_cancel':
          cancelled.push(args.id!);
          if (view.id === args.id) view = { ...view, phase: 'cancelled' };
          return;
        case 'gap_accept': {
          if (failAccept) throw new Error('Injected stale gap proposal');
          assert(
            args.acknowledgeNoMedia === true,
            'Missing no-media acknowledgment',
          );
          accepts++;
          before = structuredClone(doc);
          doc = {
            ...doc,
            revision: doc.revision + 1,
            dirty: true,
            canUndo: true,
            cues: doc.cues.map((c) => ({
              ...c,
              endMs:
                view.rows.find((r) => r.selected && r.cueId === String(c.id))
                  ?.newEndMs ?? c.endMs,
            })),
          };
          return structuredClone(doc);
        }
        case 'quality_read':
          throw new Error('Quality must remain disabled in this fixture');
        case 'document_action': {
          const action = args.action!;
          if (action.kind === 'edit') {
            if (failApply) throw new Error('Injected draft failure');
            edits++;
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
      host.textContent!.includes('No video, so shot changes weren’t checked.'),
      'Missing no-media note',
    );
    assert(
      !host.querySelector('dialog[open]'),
      'Timing review still occludes video',
    );
    await until(() => host.querySelectorAll('.ghosts .ghost').length === 2);
    assert(
      host.querySelectorAll('.timing-delta').length === 2,
      'Missing list deltas',
    );
    passed(
      'Live no-media preview leaves accepted subtitles intact and projects dashed ghosts plus list deltas',
    );
    host
      .querySelector<HTMLInputElement>('[aria-label="Include cue 1"]')!
      .click();
    await until(() =>
      [...host.querySelectorAll<HTMLButtonElement>('button')].some(
        (b) => b.textContent?.trim() === 'Apply 1 change' && !b.disabled,
      ),
    );
    await until(() => host.querySelectorAll('.ghosts .ghost').length === 1);
    assert(
      host.querySelector('.ghosts .ghost')?.getAttribute('data-preview-cue') ===
        '2',
      'Unchecked ghost survived',
    );
    host
      .querySelector<HTMLInputElement>('[aria-label="Include cue 1"]')!
      .click();
    await until(() =>
      [...host.querySelectorAll<HTMLButtonElement>('button')].some(
        (b) => b.textContent?.trim() === 'Apply 2 changes' && !b.disabled,
      ),
    );
    passed(
      'Unticking removes the change from Rust acceptance and both visible projections; reticking restores it',
    );
    failAccept = true;
    button('Apply 2 changes').click();
    await until(() =>
      host.textContent!.includes('Injected stale gap proposal'),
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
        doc.cues[0]!.endMs === 1970 &&
        doc.cues[1]!.endMs === 3020,
      'Accepted timing mismatch',
    );
    button('Close').click();
    editor.focus();
    button('Undo edit').click();
    await until(() => doc.cues[0]!.endMs === 2000);
    assert(
      JSON.stringify(doc.cues) === JSON.stringify(original.cues),
      'Undo did not restore all cues',
    );
    passed(
      'Failed acceptance preserves state; explicit acceptance reconciles one revision and one Undo restores all timings',
    );
    view = {
      ...structuredClone(sample),
      total: 205,
      changed: 0,
      skipped: 205,
      rows: Array.from({ length: 205 }, (_, i) => ({
        ...sample.rows[0]!,
        cueId: String(i + 1),
        cueIndex: i,
        newEndMs: null,
        selected: false,
        skipped: 'minimumDuration',
        explanation: 'Would violate minimum duration',
      })),
    };
    await open();
    await until(() => host.querySelectorAll('.change').length === 100);
    assert(
      button('Apply 0 changes').disabled,
      'All-skipped proposal was acceptable',
    );
    button('Next').click();
    await until(() => host.textContent!.includes('101–200 / 205'));
    assert(
      host.querySelectorAll('.change').length === 100,
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
    heading.textContent = `PASS: all ${output.children.length} gap review checks`;
  } catch (error) {
    heading.textContent = `FAIL: ${String(error)}`;
    await unmount(app);
  }
}
void run();
