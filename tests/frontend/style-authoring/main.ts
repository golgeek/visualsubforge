import { mount, unmount, tick } from 'svelte';
import StyleInspector from '../../../src/lib/StyleInspector.svelte';
import { DocumentSession } from '../../../src/lib/document-session.svelte';
import type {
  DocumentSnapshot,
  StyleOperation,
  StyleReview,
  StyleForm,
} from '../../../src/lib/client';
import {
  styledDocument,
  styleForm,
  styleDefaults,
  styleReview,
} from '../ipc-contract-samples';
import { lifecycle } from './lifecycle';
const host = document.querySelector('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
const assert = (v: unknown, m: string) => {
  if (!v) throw new Error(m);
};
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  output.append(row);
}
function button(label: string) {
  const b = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!b) throw new Error(`Missing button ${label}`);
  return b;
}
function input(label: string, value: string) {
  const element = host.querySelector<HTMLInputElement>(
    `[aria-label="${label}"]`,
  );
  if (!element) throw new Error(`Missing ${label}`);
  element.value = value;
  element.dispatchEvent(new Event('input', { bubbles: true }));
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
let doc: DocumentSnapshot = {
  ...structuredClone(styledDocument),
  generation: 1,
  revision: 0,
  dirty: false,
  canUndo: false,
  requiresSaveAs: false,
};
let before = structuredClone(doc);
let accepted = 0,
  requested = 0;
let operation: StyleOperation = { kind: 'edit', name: 'Title', fields: [] };
let view: StyleReview = structuredClone(styleReview);
let failAccept = false;
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    invoke: async (
      command: string,
      args: { operation?: StyleOperation; name?: string | null },
    ) => {
      if (command === 'style_form') {
        const form: StyleForm = structuredClone(
          args.name ? styleForm : styleDefaults,
        );
        if (args.name)
          form.fields.push({
            name: 'VendorColumn',
            value: '<b>verbatim</b>',
            editable: false,
          });
        return form;
      }
      if (command === 'style_start') {
        operation = structuredClone(args.operation!);
        requested++;
        view = {
          ...structuredClone(styleReview),
          id: requested,
          operation,
          key: {
            ...styleReview.key,
            generation: doc.generation,
            revision: doc.revision,
          },
        };
        return requested;
      }
      if (command === 'style_view') return structuredClone(view);
      if (command === 'style_cancel') {
        view = { ...view, phase: 'cancelled' };
        return;
      }
      throw new Error(`Unexpected IPC ${command}`);
    },
  },
});
const subtitles = new DocumentSession({
  chooseFile: async () => null,
  resyncAccept: async () => doc,
  gapAccept: async () => doc,
  documentAction: async (action) => {
    if (action.kind === 'undo')
      doc = { ...before, revision: doc.revision + 1, canRedo: true };
    else assert(action.kind === 'status', 'Only status and undo expected');
    return structuredClone(doc);
  },
  styleAccept: async () => {
    if (failAccept) throw new Error('Review invalidated before acceptance');
    before = structuredClone(doc);
    const next =
      operation.fields.find((f) => f.name === 'Name')?.value ?? 'Title';
    doc = {
      ...doc,
      revision: doc.revision + 1,
      dirty: true,
      canUndo: true,
      cues: doc.cues.map((c) =>
        c.style === 'Title' ? { ...c, style: next } : c,
      ),
      format: doc.format
        ? {
            ...doc.format,
            styles: doc.format.styles.map((s) =>
              s.name === 'Title'
                ? {
                    ...s,
                    name: next,
                    fields: s.fields.map(([name, value]) => [
                      name,
                      operation.fields.find((f) => f.name === name)?.value ??
                        value,
                    ]),
                  }
                : s,
            ),
          }
        : null,
    };
    return structuredClone(doc);
  },
});
let app: ReturnType<typeof mount> | null = null;
async function show() {
  app = mount(StyleInspector, {
    target: host,
    props: {
      subtitles,
      initial: 'Title',
      onclose: () => {},
      onaccepted: () => {
        accepted++;
      },
    },
  });
  await until(() => !!host.querySelector('dialog[open]'));
}
async function run() {
  try {
    await subtitles.command({ kind: 'status' });
    subtitles.select(doc.cues[0]!.id);
    subtitles.text = 'Unapplied draft';
    await show();
    assert(button('Edit style').disabled, 'Cue draft did not block authoring');
    assert(
      host.querySelector('[aria-label="Style fields"]'),
      'Draft prevented inspection',
    );
    button('Discard cue draft').click();
    await tick();
    button('Edit style').click();
    await until(() => !!host.querySelector('[aria-label="Style Fontsize"]'));
    assert(
      !host.querySelector('[aria-label="Style VendorColumn"]') &&
        host.textContent?.includes('<b>verbatim</b>') &&
        !host.querySelector('b'),
      'Unknown field not preserved as escaped read-only text',
    );
    input('Style Name', 'Heading');
    input('Style Fontsize', '42');
    await tick();
    assert(
      !doc.dirty && doc.cues[0]!.style === 'Title',
      'Form mutated acceptance',
    );
    button('Preview style changes').click();
    await until(
      () => !!host.querySelector('[aria-label="Style change review"]'),
    );
    assert(
      operation.kind === 'edit' &&
        operation.name === 'Title' &&
        operation.fields.some(
          (f) => f.name === 'Name' && f.value === 'Heading',
        ),
      'Rename did not capture original identity',
    );
    assert(
      operation.fields.length === 2,
      'Unchanged or unknown fields were rewritten',
    );
    assert(
      !button('Accept style changes').disabled,
      'Review missing acceptance',
    );
    passed(
      'Cue draft protection, escaped unknown fields and review preserve accepted state',
    );
    failAccept = true;
    button('Accept style changes').click();
    await until(() =>
      host.textContent!.includes('Review invalidated before acceptance'),
    );
    assert(accepted === 0 && !doc.dirty, 'Failed acceptance changed state');
    failAccept = false;
    button('Accept style changes').click();
    await until(() => accepted === 1);
    assert(
      subtitles.style === 'Heading' &&
        subtitles.snapshot.format?.styles.some((s) => s.name === 'Heading'),
      'Accepted style and cue not reconciled',
    );
    passed(
      'Failed acceptance retains drafts; successful acceptance reconciles style and cue together',
    );
    await unmount(app!);
    app = null;
    await subtitles.command({ kind: 'undo' });
    assert(
      subtitles.style === 'Title' && !subtitles.snapshot.dirty,
      'Undo did not restore owner projection',
    );
    await show();
    button('Copy style').click();
    await until(() => !!host.querySelector('[aria-label="Style Name"]'));
    input('Style Name', 'Copied');
    await tick();
    button('Preview style changes').click();
    await until(
      () => !!host.querySelector('[aria-label="Style change review"]'),
    );
    assert(
      (operation as StyleOperation).kind === 'create' &&
        'copyFrom' in operation &&
        operation.copyFrom === 'Title',
      'Copy omitted source style',
    );
    input('Style Fontsize', '44');
    await tick();
    assert(
      !host.querySelector('[aria-label="Style change review"]'),
      'Changed form kept old review visible',
    );
    await until(() =>
      host.textContent!.includes('Style form or subtitles changed'),
    );
    button('Discard style draft').click();
    await tick();
    assert(
      !host.querySelector('[aria-label="Style Name"]'),
      'Discard did not return to inspection',
    );
    button('New style').click();
    await until(() => !!host.querySelector('[aria-label="Style Name"]'));
    input('Style Name', 'Fresh');
    await tick();
    button('Preview style changes').click();
    await until(
      () => !!host.querySelector('[aria-label="Style change review"]'),
    );
    assert(
      (operation as StyleOperation).kind === 'create' &&
        'copyFrom' in operation &&
        operation.copyFrom === null &&
        operation.name === 'Fresh',
      'New style did not use canonical defaults',
    );
    view = { ...view, phase: 'stale', summary: null };
    await until(() => host.textContent!.includes('Style preview: stale'));
    assert(
      !host.querySelector('[aria-label="Style change review"]') &&
        accepted === 1,
      'Stale proposal retained acceptance',
    );
    passed(
      'Copy/new capture distinct sources, form changes retire reviews, and stale policy results cannot be accepted',
    );
    await unmount(app!);
    app = null;
    await lifecycle(styleForm, styleReview, doc, passed);
    heading.textContent = `PASS: all ${output.querySelectorAll('li').length} style authoring checks`;
  } catch (e) {
    heading.textContent = `FAIL: ${String(e)}`;
    console.error(e);
  } finally {
    if (app) await unmount(app);
    subtitles.dispose();
  }
}
void run();
