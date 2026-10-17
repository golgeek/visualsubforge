import { mount, unmount, tick } from 'svelte';
import App from '../../../src/App.svelte';
import {
  emptyDocument,
  emptyStatus,
  defaultPreferences,
  type DocumentSnapshot,
  type DocumentAction,
  type TextView,
  type TextOperation,
  type TextTarget,
} from '../../../src/lib/client';
import { textView as sample } from '../ipc-contract-samples';
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
  cues: sample.rows.map((r, i) => ({
    id: r.cueId,
    startMs: i * 3000 + 1001,
    endMs: i * 3000 + 2999,
    text: r.before,
  })),
};
let doc = structuredClone(original),
  before = structuredClone(original);
let view: TextView = structuredClone(sample);
let starts = 0,
  accepts = 0,
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
  const b = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === text,
  );
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
function input(label: string, value: string) {
  const e = host.querySelector<HTMLInputElement | HTMLTextAreaElement>(
    `[aria-label="${label}"]`,
  )!;
  e.value = value;
  e.dispatchEvent(new Event('input', { bubbles: true }));
}
async function open() {
  await until(() => !button('Find / Replace…').disabled);
  button('Find / Replace…').click();
  await until(() => !!host.querySelector('dialog[open]'));
}
async function mode(value: string) {
  const select = host.querySelector<HTMLSelectElement>('dialog select')!;
  select.value = value;
  select.dispatchEvent(new Event('change', { bubbles: true }));
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
    transformCallback: () => 1,
    unregisterCallback: () => {},
    invoke: async (
      command: string,
      args: {
        action?: DocumentAction;
        id?: number;
        offset?: number;
        limit?: number;
        operation?: TextOperation;
        target?: TextTarget;
        text?: string;
        start?: number;
        end?: number;
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
        case 'quality_read':
          throw new Error('Quality disabled in this fixture');
        case 'text_start': {
          starts++;
          const find = args.operation?.kind === 'find';
          view = {
            ...structuredClone(sample),
            id: starts,
            operation: args.operation!,
            changed: find ? 0 : sample.changed,
            key: {
              ...sample.key,
              generation: doc.generation,
              revision: doc.revision,
            },
            rows: sample.rows.map((r) => ({
              ...r,
              after: find ? null : r.after,
            })),
          };
          return heldStart ?? starts;
        }
        case 'text_view':
          return structuredClone(view);
        case 'text_cancel':
          cancelled.push(args.id!);
          return;
        case 'text_accept': {
          if (failAccept) throw new Error('Injected stale text preview');
          accepts++;
          before = structuredClone(doc);
          doc = {
            ...doc,
            revision: doc.revision + 1,
            dirty: true,
            canUndo: true,
            cues: doc.cues.map((c) => ({
              ...c,
              text: view.rows.find((r) => r.cueId === c.id)?.after ?? c.text,
            })),
          };
          return structuredClone(doc);
        }
        case 'text_format':
          return {
            text: `${args.text!.slice(0, args.start)}<b>${args.text!.slice(args.start, args.end)}</b>${args.text!.slice(args.end)}`,
            start: args.start! + 3,
            end: args.end! + 3,
          };
        case 'document_action': {
          const a = args.action!;
          if (a.kind === 'edit') {
            if (failApply) throw new Error('Injected draft failure');
            edits++;
            doc = {
              ...doc,
              revision: doc.revision + 1,
              dirty: true,
              cues: doc.cues.map((c) =>
                c.id === a.cueId ? { ...c, text: a.text } : c,
              ),
            };
          } else if (a.kind === 'undo')
            doc = { ...before, revision: doc.revision + 1, canRedo: true };
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
    await until(() => host.querySelectorAll('.cue-row').length === 2);
    input('Subtitle text', 'Keep this draft');
    await tick();
    await open();
    input('Find text', 'été');
    await tick();
    assert(
      button('Find matches').disabled && starts === 0,
      'Search discarded draft',
    );
    button('Cancel text tools').click();
    await until(() => !host.querySelector('dialog[open]'));
    assert(
      host.querySelector<HTMLTextAreaElement>('[aria-label="Subtitle text"]')!
        .value === 'Keep this draft' && edits === 0,
      'Cancel lost draft',
    );
    passed('Search opening and cancellation preserve an unapplied draft');
    await open();
    failApply = true;
    button('Apply cue draft').click();
    await until(() => host.textContent!.includes('Injected draft failure'));
    assert(starts === 0 && edits === 0, 'Failed Apply started search');
    button('Discard cue draft').click();
    failApply = false;
    await tick();
    button('Find matches').click();
    await until(() =>
      host
        .textContent!.replace(/\s+/g, ' ')
        .includes('3 matches · 0 changed cues'),
    );
    assert(
      button('Accept text repairs').disabled && accepts === 0,
      'Find enabled mutation',
    );
    button('Go to match 2 in cue 1').click();
    await until(
      () =>
        !host.querySelector('dialog[open]') &&
        document.activeElement?.getAttribute('aria-label') === 'Subtitle text',
    );
    const editor = host.querySelector<HTMLTextAreaElement>(
      '[aria-label="Subtitle text"]',
    )!;
    assert(
      editor.value.slice(editor.selectionStart, editor.selectionEnd) === 'été',
      'Source selection not preserved',
    );
    passed(
      'Failed draft Apply blocks searching; explicit discard permits protected Unicode match navigation',
    );
    await open();
    await mode('replace');
    input('Replace with', 'summer');
    await tick();
    button('Preview text changes').click();
    await until(() => !button('Accept Replace All').disabled);
    assert(doc.revision === original.revision, 'Preview edited document');
    input('Replace with', 'winter');
    await tick();
    assert(
      button('Accept Replace All').disabled,
      'Changed input accepted old proposal',
    );
    input('Replace with', 'summer');
    await tick();
    // Poll may retire the old input capture; explicitly create a current preview.
    button('Preview text changes').click();
    await until(() => !button('Accept Replace All').disabled);
    failAccept = true;
    button('Accept Replace All').click();
    await until(() =>
      host.textContent!.includes('Injected stale text preview'),
    );
    assert(
      accepts === 0 && doc.revision === original.revision,
      'Failed acceptance changed state',
    );
    failAccept = false;
    button('Accept Replace All').click();
    await until(() => !host.querySelector('dialog[open]'));
    assert(
      accepts === 1 &&
        doc.cues[0]!.text === sample.rows[0]!.after &&
        doc.cues[1]!.text === sample.rows[1]!.after,
      'Batch not reconciled',
    );
    await until(() => !button('Find / Replace…').disabled);
    button('Undo edit').click();
    await until(() => doc.revision === original.revision + 2);
    assert(
      JSON.stringify(doc.cues) === JSON.stringify(original.cues),
      'Undo did not restore complete batch',
    );
    passed(
      'Preview is read-only; changed inputs and failed acceptance preserve state; batch replies use the document owner',
    );
    await until(() => !button('Bold').disabled);
    editor.focus();
    editor.setSelectionRange(9, 12);
    editor.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'b',
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
    await until(() => editor.value.includes('<b>ÉTÉ</b>'));
    assert(
      doc.cues[0]!.text === original.cues[0]!.text && edits === 0,
      'Formatting committed a draft implicitly',
    );
    passed(
      'Formatting shortcut changes only the draft and restores the selected text',
    );
    button('Revert draft').click();
    await tick();
    await open();
    let release!: (id: number) => void;
    heldStart = new Promise((r) => {
      release = r;
    });
    button('Preview text changes').click();
    await until(() => starts >= 4);
    button('Close text tools').click();
    await until(() => !host.querySelector('dialog[open]'));
    const lateId = starts;
    release(lateId);
    await until(() => cancelled.includes(lateId));
    passed(
      'Closing during admission cancels the eventual job and cannot reopen the dialog',
    );
    heldStart = null;
    await until(() => !button('Find / Replace…').disabled);
    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'f',
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
    await until(() => !!host.querySelector('dialog[open]'));
    assert(
      host.querySelector<HTMLSelectElement>('dialog[open] select')!.value ===
        'find',
      'Find shortcut opened wrong mode',
    );
    button('Close text tools').click();
    await until(() => !button('Find / Replace…').disabled);
    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'h',
        metaKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
    await tick();
    assert(!host.querySelector('dialog[open]'), 'Cmd+H was intercepted');
    window.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'h',
        metaKey: true,
        shiftKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
    await until(() => !!host.querySelector('dialog[open]'));
    assert(
      host.querySelector<HTMLSelectElement>('dialog[open] select')!.value ===
        'replace',
      'Replace shortcut opened wrong mode',
    );
    button('Close text tools').click();
    await until(() => !button('Find / Replace…').disabled);
    passed('Find and replace keyboard entry preserves the Mac Cmd+H shortcut');
    await lifecycle(sample, original, passed);
    const count = output.querySelectorAll('li').length;
    heading.textContent = `PASS: all ${count} text editing checks`;
  } catch (e) {
    heading.textContent = `FAIL: ${String(e)}`;
    console.error(e);
  } finally {
    await unmount(app);
  }
}
void run();
