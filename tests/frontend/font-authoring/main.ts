import { mount, unmount, tick } from 'svelte';
import FontDialog from '../../../src/lib/FontDialog.svelte';
import { FontSession } from '../../../src/lib/font-session.svelte';
import { DocumentSession } from '../../../src/lib/document-session.svelte';
import type {
  DocumentSnapshot,
  FontReview,
  FontOperation,
} from '../../../src/lib/client';
import {
  styledDocument,
  fontReview,
  fontInventory,
} from '../ipc-contract-samples';
const host = document.querySelector('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  output.append(row);
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
let operation: FontOperation = { kind: 'inspect' };
let view: FontReview = structuredClone(fontInventory);
let failAccept = false,
  pickerCancel = false;
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    invoke: async (
      command: string,
      args: { operation?: FontOperation; kind?: string },
    ) => {
      if (command === 'choose_file') {
        assert(args.kind === 'font', 'Wrong chooser');
        return pickerCancel ? null : '/fixture/New.otf';
      }
      if (command === 'font_start') {
        operation = structuredClone(args.operation!);
        requested++;
        view = {
          ...structuredClone(
            operation.kind === 'inspect' ? fontInventory : fontReview,
          ),
          id: requested,
          operation,
          key: {
            ...fontReview.key,
            generation: doc.generation,
            revision: doc.revision,
          },
        };
        if (operation.kind === 'inspect')
          view.inventory!.entries.push({
            ...structuredClone(view.inventory!.entries[0]!),
            id: 999,
            name: '<b>opaque.ttf</b>',
            font: null,
            problem: 'Unsupported font structure',
          });
        // The initial inventory is fully supported; later explicit refresh tests
        // unsupported rows without ever inserting markup into the DOM.
        if (requested === 1) view.inventory!.entries.pop();
        if (view.inventory)
          view.inventory.total = view.inventory.entries.length;
        if (operation.kind === 'replace' || operation.kind === 'remove')
          view.summary!.before = structuredClone(fontReview.summary!.after);
        if (operation.kind === 'remove') {
          view.summary!.action = 'remove';
          view.summary!.after = null;
        }
        if (operation.kind === 'replace') view.summary!.action = 'replace';
        return requested;
      }
      if (command === 'font_view') return structuredClone(view);
      if (command === 'font_cancel') {
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
    else assert(action.kind === 'status', 'Only status and Undo expected');
    return structuredClone(doc);
  },
  fontAccept: async () => {
    if (failAccept)
      throw new Error('Font review invalidated before acceptance');
    before = structuredClone(doc);
    doc = { ...doc, revision: doc.revision + 1, dirty: true, canUndo: true };
    return structuredClone(doc);
  },
});
let app: ReturnType<typeof mount> | null = null;
async function show() {
  app = mount(FontDialog, {
    target: host,
    props: {
      subtitles,
      onclose: () => {},
      onaccepted: () => {
        accepted++;
      },
    },
  });
  await until(() => !!host.querySelector('[aria-label="Attachment details"]'));
}

async function lifecycle() {
  let snapshot = structuredClone(doc);
  let releaseAccept!: (v: DocumentSnapshot) => void;
  const owner = new DocumentSession({
    chooseFile: async () => null,
    documentAction: async () => snapshot,
    resyncAccept: async () => snapshot,
    gapAccept: async () => snapshot,
    fontAccept: () =>
      new Promise((resolve) => {
        releaseAccept = resolve;
      }),
  });
  await owner.command({ kind: 'status' });
  let heldPick: Promise<string | null> | null = null,
    releasePick!: (v: string | null) => void;
  let heldStart: Promise<number> | null = null,
    releaseStart!: (v: number) => void;
  let heldView: Promise<FontReview> | null = null,
    releaseView!: (v: FontReview) => void;
  let op: FontOperation = { kind: 'inspect' };
  const cancels: number[] = [];
  const api = {
    chooseFile: async () => heldPick ?? '/fixture/New.otf',
    fontStart: async (
      _doc: Pick<DocumentSnapshot, 'generation' | 'revision'>,
      operation: FontOperation,
    ) => {
      op = operation;
      return heldStart ?? 41;
    },
    fontView: async () =>
      heldView ?? {
        ...structuredClone(op.kind === 'inspect' ? fontInventory : fontReview),
        operation: op,
      },
    fontCancel: async (id: number) => {
      cancels.push(id);
    },
  };
  const session = new FontSession(owner, api);
  await session.inspect();
  heldPick = new Promise((resolve) => {
    releasePick = resolve;
  });
  const pick = session.pick();
  assert(!session.canClose, 'Native picker did not protect lifetime');
  snapshot = { ...snapshot, revision: snapshot.revision + 1 };
  await owner.command({ kind: 'status' });
  releasePick('/fixture/New.otf');
  await pick;
  heldPick = null;
  assert(
    !session.canAccept && session.error.includes('changed while choosing'),
    'Late file choice escaped stale protection',
  );
  passed('File choice rechecks document identity and protects dialog lifetime');
  await session.inspect();
  await session.pick();
  assert(session.canAccept, 'Completed font proposal not ready');
  heldView = new Promise((resolve) => {
    releaseView = resolve;
  });
  const poll = session.poll();
  owner.select(owner.snapshot.cues[0]!.id);
  owner.text = 'unapplied draft';
  assert(!session.canAccept, 'New cue draft kept acceptance enabled');
  releaseView(fontReview);
  await poll;
  heldView = null;
  assert(
    !session.view && cancels.includes(41),
    'Late review survived a cue draft',
  );
  owner.revert();
  passed(
    'Cue drafts immediately block acceptance and retire late review results',
  );
  await session.inspect();
  await session.pick();
  const accepting = session.accept();
  assert(
    !session.canClose && !session.canStart,
    'Acceptance did not protect modal lifetime',
  );
  releaseAccept({ ...snapshot, revision: snapshot.revision + 1, dirty: true });
  assert(await accepting, 'Accepted owner projection not reconciled');
  assert(
    owner.snapshot.revision === snapshot.revision + 1,
    'Accepted revision missing',
  );
  passed(
    'One accepted reply flows through the document owner while close stays disabled',
  );
  session.dispose();
  const late = new FontSession(owner, api);
  heldStart = new Promise((resolve) => {
    releaseStart = resolve;
  });
  const start = late.inspect();
  late.dispose();
  releaseStart(777);
  await start;
  heldStart = null;
  assert(
    cancels.includes(777) && !late.view,
    'Disposed session adopted late job',
  );
  passed('Unmount cancels a late job identity and discards its result');
  const disposed = new FontSession(owner, api);
  await disposed.inspect();
  await disposed.pick();
  const accept = disposed.accept();
  disposed.dispose();
  releaseAccept({
    ...owner.snapshot,
    revision: owner.snapshot.revision + 1,
    dirty: true,
  });
  assert(
    !(await accept),
    'Disposed dialog emitted success after late acceptance',
  );
  passed(
    'Late acceptance still reconciles owner state without reviving a closed dialog',
  );
  owner.dispose();
}
async function run() {
  try {
    await subtitles.command({ kind: 'status' });
    subtitles.select(doc.cues[0]!.id);
    subtitles.text = 'Unapplied draft';
    await show();
    assert(
      button('Add font…').disabled,
      'Cue draft did not block attachment edits',
    );
    button('Discard cue draft').click();
    await tick();
    pickerCancel = true;
    button('Add font…').click();
    await until(() => !button('Add font…').disabled);
    assert(
      requested === 1 && !doc.dirty,
      'Cancelled file picker changed state or started a proposal',
    );
    pickerCancel = false;
    button('Replace font…').click();
    await until(
      () => !!host.querySelector('[aria-label="Font change review"]'),
    );
    assert(
      operation.kind === 'replace',
      'Replacement did not capture attachment identity',
    );
    assert(
      !doc.dirty && !button('Accept font change').disabled,
      'Review mutated document or omitted acceptance',
    );
    passed(
      'Inspection works with cue drafts; picker cancellation and replacement preserve accepted state',
    );
    failAccept = true;
    button('Accept font change').click();
    await until(() => host.textContent!.includes('Font review invalidated'));
    assert(accepted === 0 && !doc.dirty, 'Failed acceptance mutated state');
    failAccept = false;
    button('Accept font change').click();
    await until(() => accepted === 1);
    assert(
      subtitles.snapshot.revision === 1 && subtitles.snapshot.dirty,
      'Accepted metadata edit not reconciled',
    );
    await unmount(app!);
    app = null;
    await subtitles.command({ kind: 'undo' });
    assert(!subtitles.snapshot.dirty, 'Undo did not reconcile saved state');
    passed(
      'Failed acceptance preserves state; successful acceptance and Undo reconcile one document snapshot',
    );
    await show();
    assert(
      button('Add font…').disabled && button('Replace font…').disabled,
      'Unsupported inventory allowed writes',
    );
    const select = host.querySelector<HTMLSelectElement>(
      '[aria-label="Attachment"]',
    )!;
    select.selectedIndex = 1;
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await tick();
    assert(
      host.textContent!.includes('<b>opaque.ttf</b>') &&
        !host.querySelector('b') &&
        button('Preview removal').disabled,
      'Unsupported row was not safely displayed read-only',
    );
    select.selectedIndex = 0;
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await tick();
    button('Preview removal').click();
    await until(
      () => !!host.querySelector('[aria-label="Font change review"]'),
    );
    assert(
      host.textContent!.includes('may fall back') &&
        operation.kind === 'remove',
      'Supported removal among opaque peers unavailable',
    );
    view = { ...view, phase: 'stale', summary: null };
    await until(() => host.textContent!.includes('Font review: stale'));
    assert(
      !host.querySelector('[aria-label="Font change review"]'),
      'Stale review remained actionable',
    );
    passed(
      'Unsupported rows are escaped and preserved; supported removal is explicit and stale reviews are retired',
    );
    await unmount(app!);
    app = null;
    await lifecycle();
    heading.textContent = `PASS: all ${output.querySelectorAll('li').length} font authoring checks`;
  } catch (e) {
    heading.textContent = `FAIL: ${String(e)}`;
    console.error(e);
  } finally {
    if (app) await unmount(app);
    subtitles.dispose();
  }
}
void run();
