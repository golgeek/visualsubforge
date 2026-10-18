import { PluginSession } from '../../../src/lib/plugin-session.svelte';
import { DocumentSession } from '../../../src/lib/document-session.svelte';
import { pluginReview, styledDocument } from '../ipc-contract-samples';
import type {
  PluginReview,
  PluginInput,
  DocumentSnapshot,
  PluginTextChunk,
} from '../../../src/lib/client';

export async function lifecycle(passed: (message: string) => void) {
  const assert = (v: unknown, m: string) => {
    if (!v) throw new Error(m);
  };
  let snapshot: DocumentSnapshot = structuredClone(styledDocument);
  let statusFails = false,
    acceptFails = false,
    commitBeforeFailure = false;
  let accepts = 0;
  let acceptedInput: PluginInput | null = null;
  let releaseAccept: ((value: DocumentSnapshot) => void) | null = null;
  let holdAccept = false;
  const owner = new DocumentSession({
    chooseFile: async () => null,
    resyncAccept: async () => snapshot,
    gapAccept: async () => snapshot,
    documentAction: async () => {
      if (statusFails) throw new Error('offline');
      return structuredClone(snapshot);
    },
    pluginAccept: async (_id, input) => {
      accepts++;
      acceptedInput = structuredClone(input);
      if (holdAccept)
        return new Promise((resolve) => {
          releaseAccept = resolve;
        });
      if (commitBeforeFailure)
        snapshot = {
          ...snapshot,
          revision: snapshot.revision + 1,
          dirty: true,
        };
      if (acceptFails) throw new Error('lost reply');
      return structuredClone(snapshot);
    },
  });
  await owner.command({ kind: 'status' });
  let context = {
    session: 7,
    quality: 'profile/scene',
    cursorMs: 1234 as number | null,
  };
  let view: PluginReview = structuredClone(pluginReview);
  let nextId = 0;
  let heldStart: Promise<number> | null = null;
  let heldView: Promise<PluginReview> | null = null;
  let heldText: Promise<PluginTextChunk> | null = null;
  let picker: Promise<string | null> | null = null;
  const cancelled: number[] = [];
  const api = {
    chooseFile: async () => picker ?? '/fixture/manifest.json',
    pluginStart: async () => {
      view = { ...view, id: ++nextId, phase: 'reviewing' };
      return heldStart ?? nextId;
    },
    pluginView: async () => heldView ?? structuredClone(view),
    pluginCancel: async (id: number) => {
      cancelled.push(id);
      if (id === view.id) view = { ...view, phase: 'cancelled' };
    },
    pluginText: async () =>
      heldText ?? { offset: 0, nextOffset: 4, totalBytes: 4, text: '😀' },
  };
  const session = new PluginSession(owner, () => context, api);
  session.path = '/fixture/manifest.json';
  await session.start('fix');
  assert(session.canAccept, 'Initial fix review unavailable');
  for (const change of [
    () => {
      context = { ...context, session: context.session + 1 };
    },
    () => {
      context = { ...context, quality: context.quality + '/new' };
    },
    () => {
      context = { ...context, cursorMs: 2222 };
    },
    () => {
      owner.select(owner.snapshot.cues[1]!.id);
    },
    () => {
      session.settings = [{ key: '_setting', type: 'number', value: '3.5' }];
    },
    () => {
      owner.text += ' draft';
    },
  ]) {
    change();
    assert(!session.canAccept, 'Changed context kept acceptance enabled');
    await session.poll();
    assert(
      !session.view && cancelled.includes(nextId),
      'Changed context retained private review',
    );
    owner.revert();
    await session.start('fix');
    assert(session.canAccept, `Review did not restart: ${session.error}`);
  }
  passed(
    'Selection, draft, settings, cursor, media and profile/scene changes invalidate reviews',
  );

  let releaseView!: (value: PluginReview) => void;
  heldView = new Promise((resolve) => {
    releaseView = resolve;
  });
  const polling = session.poll();
  const cancelling = session.cancel();
  releaseView(structuredClone(pluginReview));
  await Promise.all([polling, cancelling]);
  heldView = null;
  // A delayed transport can return an older phase even for cancel's read. The
  // next authoritative view must settle it; stale polling must never win later.
  await session.poll();
  assert(
    session.view?.phase === 'cancelled' && !session.canAccept,
    'Cancel was revived by old polling',
  );
  passed('Cancellation retires an in-flight polling response');

  await session.start('fix');
  heldView = new Promise((resolve) => {
    releaseView = resolve;
  });
  const paging = session.page(50, 0);
  snapshot = { ...snapshot, revision: snapshot.revision + 1 };
  await owner.command({ kind: 'status' });
  releaseView(structuredClone(view));
  await paging;
  heldView = null;
  assert(
    !session.view && !session.canAccept,
    'Late page revived a replaced document',
  );
  await session.start('fix');
  let releaseText!: (value: PluginTextChunk) => void;
  heldText = new Promise((resolve) => {
    releaseText = resolve;
  });
  const reading = session.text(0, false);
  context = { ...context, quality: 'changed while reading' };
  releaseText({ offset: 0, nextOffset: 4, totalBytes: 4, text: '😀' });
  await reading;
  heldText = null;
  assert(
    !session.detail && !session.view,
    'Late text chunk revived a stale review',
  );
  passed(
    'Delayed pages and full-text chunks cannot cross document/context changes',
  );

  let releasePicker!: (value: string | null) => void;
  picker = new Promise((resolve) => {
    releasePicker = resolve;
  });
  const picking = session.pick();
  assert(
    !session.canClose && !session.canStart,
    'Picker allowed close or a second operation',
  );
  releasePicker(null);
  await picking;
  picker = null;
  assert(
    session.path === '/fixture/manifest.json' && session.canClose,
    'Cancelled picker replaced package',
  );
  session.settings = [{ key: '__proto__', type: 'text', value: 'x' }];
  assert(!session.canRun, 'Reserved setting was accepted');
  session.settings = [
    { key: '0_name', type: 'text', value: '😀'.repeat(1025) },
  ];
  assert(!session.canRun, 'UTF-8 setting limit counted UTF-16 units');
  session.settings = [{ key: '0_name', type: 'text', value: 'Été' }];
  assert(session.canRun, 'Allowed numeric/underscore setting name was refused');
  passed(
    'Chooser cancellation preserves the package; settings follow scalar and UTF-8 limits',
  );

  await session.start('fix');
  holdAccept = true;
  const accepting = session.accept();
  assert(
    !session.canClose && !session.canStart,
    'Acceptance allowed another operation',
  );
  await session.accept();
  assert(accepts === 1, 'Duplicate acceptance sent');
  assert(acceptedInput !== null, 'Acceptance omitted current invocation');
  const finish = releaseAccept as unknown as (value: DocumentSnapshot) => void;
  finish({ ...snapshot, revision: snapshot.revision + 1 });
  assert(await accepting, 'Accepted snapshot not adopted');
  holdAccept = false;
  snapshot = structuredClone(owner.snapshot);
  assert(
    !session.view && !session.canAccept,
    'Accepted review remained actionable',
  );
  passed(
    'Acceptance serializes through DocumentSession and adopts its single snapshot',
  );

  await session.start('fix');
  acceptFails = true;
  commitBeforeFailure = true;
  const oldRevision = owner.snapshot.revision;
  assert(!(await session.accept()), 'Lost reply claimed confirmed acceptance');
  assert(
    owner.snapshot.revision === oldRevision + 1 && !session.uncertain,
    'Lost reply did not reconcile committed state',
  );
  await session.start('fix');
  statusFails = true;
  await session.accept();
  assert(
    session.uncertain && !session.canClose && !session.canRun,
    'Unknown commit state allowed continuation',
  );
  statusFails = false;
  await session.reconcile();
  assert(
    !session.uncertain && session.canClose,
    'Retry did not release reconciliation gate',
  );
  passed(
    'Lost acceptance replies refresh committed state; failed refresh blocks until an explicit retry succeeds',
  );

  await session.start('fix');
  let releaseStart!: (id: number) => void;
  heldStart = new Promise((resolve) => {
    releaseStart = resolve;
  });
  const starting = session.start('fix');
  // The prior review must first be cancelled; let that await complete.
  await Promise.resolve();
  await Promise.resolve();
  session.dispose();
  releaseStart(999);
  await starting;
  assert(
    cancelled.includes(999) && !session.view,
    'Late start leaked after close',
  );
  owner.dispose();
  passed(
    'Closing during delayed startup cancels its eventual job without restoring the dialog',
  );
}
