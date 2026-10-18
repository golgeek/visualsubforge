import { mount, unmount, tick } from 'svelte';
import PluginDialog from '../../../src/lib/PluginDialog.svelte';
import { DocumentSession } from '../../../src/lib/document-session.svelte';
import {
  emptyStatus,
  type DocumentSnapshot,
  type PluginInput,
  type PluginReview,
} from '../../../src/lib/client';
import { styledDocument, pluginReview } from '../ipc-contract-samples';
import { lifecycle } from './lifecycle';
const host = document.querySelector('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
function assert(v: unknown, m: string) {
  if (!v) throw new Error(m);
}
function passed(message: string) {
  const li = document.createElement('li');
  li.textContent = `PASS: ${message}`;
  output.append(li);
}
function button(label: string) {
  const b = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!b) throw new Error(`Missing ${label}`);
  return b;
}
async function until(check: () => boolean) {
  await tick();
  const deadline = performance.now() + 6000;
  while (!check()) {
    assert(performance.now() < deadline, `Timed out: ${host.textContent}`);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
let doc: DocumentSnapshot = structuredClone(styledDocument);
let input: PluginInput | null = null;
let view: PluginReview = structuredClone(pluginReview);
let cancelled = false,
  pickerCancel = true,
  accepted = 0,
  starts = 0;
const full = '😀<b>Été</b>';
const bytes = new TextEncoder().encode(full);
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    invoke: async (
      command: string,
      args: {
        kind?: string;
        input?: PluginInput;
        findingsOffset?: number;
        changesOffset?: number;
        offset?: number;
      },
    ) => {
      if (command === 'choose_file') {
        assert(args.kind === 'plugin', 'Wrong chooser');
        return pickerCancel ? null : '/fixtures/plugin/manifest.json';
      }
      if (command === 'plugin_start') {
        starts++;
        input = structuredClone(args.input!);
        cancelled = false;
        view = {
          ...structuredClone(pluginReview),
          id: starts,
          mode: input.mode,
          phase: 'reviewing',
          settings: { flag: true, name: 'Été', number: 3.5 },
          logs: ['<b>literal log</b>'],
          findingsTotal: 51,
          findings: Array.from({ length: 51 }, (_, i) => ({
            cueId: String(i + 1),
            message: '<b>literal finding</b>',
          })),
          changesTotal: input.mode === 'fix' ? 2 : 0,
          rows: input.mode === 'fix' ? structuredClone(pluginReview.rows) : [],
        };
        if (view.rows.length) {
          view.rows[0]!.before.text = {
            text: '😀',
            totalBytes: bytes.length,
            truncated: true,
          };
          view.rows[1]!.outsideScope = true;
          view.rows[1]!.warning = 'Markup is preserved in the raw text.';
        }
        return starts;
      }
      if (command === 'plugin_view')
        return {
          ...structuredClone(view),
          phase: cancelled ? 'cancelled' : view.phase,
          findingsOffset: args.findingsOffset ?? 0,
          changesOffset: args.changesOffset ?? 0,
          findings: view.findings.slice(
            args.findingsOffset ?? 0,
            (args.findingsOffset ?? 0) + 50,
          ),
        };
      if (command === 'plugin_cancel') {
        cancelled = true;
        return;
      }
      if (command === 'plugin_text') {
        const offset = args.offset ?? 0;
        const end = offset === 0 ? 4 : bytes.length;
        return {
          offset,
          nextOffset: end,
          totalBytes: bytes.length,
          text: new TextDecoder().decode(bytes.slice(offset, end)),
        };
      }
      throw new Error(`Unexpected IPC ${command}`);
    },
  },
});
const subtitles = new DocumentSession({
  chooseFile: async () => null,
  resyncAccept: async () => doc,
  gapAccept: async () => doc,
  documentAction: async () => structuredClone(doc),
  pluginAccept: async (_id, current) => {
    assert(
      JSON.stringify(current) === JSON.stringify(input),
      'Acceptance changed invocation',
    );
    doc = { ...doc, revision: doc.revision + 1, dirty: true, canUndo: true };
    return structuredClone(doc);
  },
});
let app: ReturnType<typeof mount> | null = null;
async function run() {
  try {
    await subtitles.command({ kind: 'status' });
    app = mount(PluginDialog, {
      target: host,
      props: {
        subtitles,
        media: () => emptyStatus,
        context: () => 'profile/scene',
        onclose: () => {},
        onaccepted: () => {
          accepted++;
        },
      },
    });
    await tick();
    assert(button('Run checker').disabled, 'Running without a package enabled');
    button('Choose plugin…').click();
    await until(() => !button('Choose plugin…').disabled);
    assert(
      button('Run checker').disabled,
      'Cancelled chooser selected a package',
    );
    pickerCancel = false;
    button('Choose plugin…').click();
    await until(() => !button('Run checker').disabled);
    subtitles.text += ' draft';
    await tick();
    assert(
      button('Run checker').disabled && button('Preview plugin fixes').disabled,
      'Draft was silently included',
    );
    button('Discard cue draft').click();
    await tick();
    button('Run checker').click();
    await until(() => !!host.querySelector('[aria-label="Plugin findings"]'));
    assert(
      !host.textContent!.includes('Accept plugin changes') &&
        !host.querySelector('b'),
      'Checker mutated or injected markup',
    );
    assert(host.textContent!.includes('1–50 of 51'), 'Findings were not paged');
    button('Next findings').click();
    await until(() => host.textContent!.includes('51–51 of 51'));
    assert(button('Next findings').disabled, 'Last findings page did not stop');
    passed(
      'Package chooser, draft protection, read-only checking, escaped findings and pagination work',
    );
    const details = host.querySelector(
      'fieldset details',
    ) as HTMLDetailsElement;
    details.open = true;
    await tick();
    button('Edit effective settings').click();
    await tick();
    assert(
      host.querySelector('[aria-label="Setting 1 type"]'),
      'Effective settings cannot be edited',
    );
    button('Preview plugin fixes').click();
    await until(
      () =>
        host.textContent!.includes('Accept plugin changes') &&
        !button('Accept plugin changes').disabled,
    );
    assert(
      host.textContent!.includes('Outside selected scope') &&
        host.textContent!.includes('across all pages'),
      'Neighbor mutation was hidden',
    );
    assert(
      host.textContent!.includes('{\\i1}Été'),
      'Raw ASS tags missing from review',
    );
    button('Read full before text for cue 1').click();
    await until(() => !!host.querySelector('[aria-label="Full plugin text"]'));
    assert(
      !button('Next text chunk').disabled,
      'Full-text continuation missing',
    );
    button('Next text chunk').click();
    await until(() => button('Next text chunk').disabled);
    assert(
      host
        .querySelector('[aria-label="Full plugin text"]')!
        .textContent!.includes('<b>Été</b>') && !host.querySelector('b'),
      'Full text was lost or interpreted as markup',
    );
    button('First text chunk').click();
    await until(() => !button('Next text chunk').disabled);
    passed(
      'Fix review exposes raw tags, neighbor changes, typed settings and complete bounded Unicode text',
    );
    button('Accept plugin changes').click();
    await until(() => accepted === 1);
    assert(
      subtitles.snapshot.canUndo &&
        !host.textContent!.includes('Accept plugin changes'),
      'Accepted snapshot or review retirement missing',
    );
    passed('Accepted fixes flow through DocumentSession and retire the review');
    await unmount(app);
    app = null;
    await lifecycle(passed);
    heading.textContent = `PASS: all ${output.querySelectorAll('li').length} plugin checks`;
  } catch (e) {
    heading.textContent = `FAIL: ${String(e)}`;
    console.error(e);
  } finally {
    if (app) await unmount(app);
    subtitles.dispose();
  }
}
void run();
