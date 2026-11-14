import { tick } from 'svelte';
import { CueFixes } from '../../../src/lib/quality/cue-fixes.svelte';
import {
  CueEditing,
  AUTOCOMMIT_DELAY_MS,
} from '../../../src/lib/cues/cue-editing.svelte';
import { DocumentSession } from '../../../src/lib/document/document-session.svelte';
import {
  DocumentFailure,
  emptyDocument,
  emptyStatus,
  type DocumentSnapshot,
} from '../../../src/lib/client';
const output = document.querySelector('#checks')!;
const heading = document.querySelector('#result')!;
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const li = document.createElement('li');
  li.textContent = `PASS: ${message}`;
  output.append(li);
}
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
async function pause() {
  await tick();
  await sleep(AUTOCOMMIT_DELAY_MS + 100);
  await tick();
}
async function until(check: () => boolean) {
  for (let n = 0; n < 100 && !check(); n++) await sleep(10);
  assert(check(), 'Request did not settle');
}
function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => (resolve = r));
  return { promise, resolve };
}
async function fixture() {
  let current: DocumentSnapshot = {
    ...emptyDocument,
    loaded: true,
    generation: 1,
    revision: 1,
    cues: [
      { id: 1, startMs: 1000, endMs: 3000, text: 'base' },
      { id: 2, startMs: 4000, endMs: 6000, text: 'other' },
    ],
  };
  const requests: { text: string; session: string | null }[] = [];
  let mode = 'success';
  let reads = 0;
  let readGate: ReturnType<typeof deferred> | null = null;
  let left = 0;
  let gate: ReturnType<typeof deferred> | null = null;
  const session = new DocumentSession({
    documentAction: async (action, version, textSession = null) => {
      if (action.kind === 'status') {
        reads++;
        if (mode === 'offline') throw new Error('Offline');
        await readGate?.promise;
        return structuredClone(current);
      }
      assert(action.kind === 'edit', 'Unexpected mutation');
      if (action.kind !== 'edit') throw new Error('Unexpected mutation');
      requests.push({ text: action.text, session: textSession });
      await gate?.promise;
      if (mode === 'rejected')
        throw new DocumentFailure({
          kind: 'rejected',
          message: 'Invalid edit',
        });
      if (mode === 'offline') throw new Error('Lost reply');
      assert(version?.revision === current.revision, 'Stale mutation');
      current = {
        ...current,
        revision: current.revision + 1,
        canUndo: true,
        dirty: true,
        cues: current.cues.map((c) =>
          c.id === action.cueId
            ? {
                ...c,
                text: action.text,
                startMs: action.startMs,
                endMs: action.endMs,
              }
            : c,
        ),
      };
      return structuredClone(current);
    },
    chooseFile: async () => {
      throw new Error('Unexpected picker');
    },
    gapAccept: async () => current,
    resyncAccept: async () => current,
  });
  await session.command({ kind: 'status' });
  session.select(1);
  let editing!: CueEditing;
  const cleanup = $effect.root(() => {
    editing = new CueEditing(
      session,
      { status: emptyStatus, pending: false },
      { range: null },
      { canCommit: () => true, leaveEditor: () => left++ },
    );
  });
  await tick();
  editing.focusText();
  return {
    session,
    editing,
    requests,
    reads: () => reads,
    left: () => left,
    mode: (value: string) => (mode = value),
    holdRead: () => {
      readGate = deferred();
      return readGate;
    },
    hold: () => {
      gate = deferred();
      return gate;
    },
    dispose: () => {
      editing.dispose();
      session.dispose();
      cleanup();
    },
  };
}
async function fixesLifecycle() {
  type Input = {
    document: DocumentSnapshot;
    cueId: number;
    session: number;
    context: string;
  };
  for (const phase of ['start', 'view']) {
    const f = await fixture();
    let input = $state.raw<Input | null>({
      document: f.session.snapshot,
      cueId: 1,
      session: 1,
      context: 'first',
    });
    let started = 0;
    let reads = 0;
    const cancelled: number[] = [];
    const gate = deferred();
    let owner!: CueFixes;
    const cleanup = $effect.root(() => {
      owner = new CueFixes(
        () => input,
        { acceptCueFixes: async () => true },
        {
          start: async () => {
            started++;
            if (phase === 'start') await gate.promise;
            return 99;
          },
          view: async () => {
            reads++;
            if (phase === 'view') await gate.promise;
            return {
              id: 99,
              key: { generation: 1, revision: 1, epoch: 1, loaded: true },
              cueId: '1',
              phase: 'reviewing',
              fixed: 2,
              remaining: 0,
              error: null,
            };
          },
          cancel: async (id) => {
            cancelled.push(id);
          },
        },
      );
    });
    await until(() => (phase === 'start' ? started === 1 : reads === 1));
    owner.dispose();
    cleanup();
    gate.resolve();
    await sleep(10);
    assert(
      cancelled.includes(99) && owner.count === 0,
      `Disposed fixes ${phase} republished or leaked admission`,
    );
    input = null;
    f.dispose();
    passed(
      `Cue fixes disposal during ${phase} cancels late work and suppresses publication`,
    );
  }
  {
    const f = await fixture();
    let input = $state.raw<Input | null>({
      document: f.session.snapshot,
      cueId: 1,
      session: 1,
      context: 'first',
    });
    let started = 0;
    const cancelled: number[] = [];
    const gate = deferred();
    let owner!: CueFixes;
    let active = 0;
    let maximum = 0;
    const cleanup = $effect.root(() => {
      owner = new CueFixes(
        () => input,
        { acceptCueFixes: async () => true },
        {
          start: async (_session, _document, cueId) => {
            started++;
            active++;
            maximum = Math.max(maximum, active);
            if (cueId === 1) await gate.promise;
            active--;
            return cueId;
          },
          view: async (id) => ({
            id,
            key: { generation: 1, revision: 1, epoch: 1, loaded: true },
            cueId: String(id),
            phase: 'reviewing',
            fixed: id,
            remaining: 0,
            error: null,
          }),
          cancel: async (id) => {
            cancelled.push(id);
          },
        },
      );
    });
    await until(() => started === 1);
    input = { ...input!, cueId: 2, context: 'second' };
    await tick();
    gate.resolve();
    await until(() => owner.count === 2);
    assert(
      maximum === 1 && cancelled.includes(1) && owner.view?.cueId === '2',
      'Fix replacement raced or published old cue',
    );
    input = null;
    await tick();
    assert(
      owner.count === 0 && cancelled.includes(2),
      'Modal/disabled input retained fix',
    );
    owner.dispose();
    cleanup();
    f.dispose();
    passed(
      'Cue fixes serialize admission, retire obsolete selection and clear on modal gating',
    );
  }
  {
    const f = await fixture();
    const input = {
      document: f.session.snapshot,
      cueId: 1,
      session: 1,
      context: 'current',
    };
    const gate = deferred();
    let accepted = 0;
    let owner!: CueFixes;
    const cleanup = $effect.root(() => {
      owner = new CueFixes(
        () => input,
        {
          acceptCueFixes: async () => {
            accepted++;
            await gate.promise;
            return true;
          },
        },
        {
          start: async () => 1,
          view: async () => ({
            id: 1,
            key: { generation: 1, revision: 1, epoch: 1, loaded: true },
            cueId: '1',
            phase: 'reviewing',
            fixed: 1,
            remaining: 0,
            error: null,
          }),
          cancel: async () => {},
        },
      );
    });
    await until(() => owner.count === 1);
    const accepting = owner.accept();
    assert(
      !(await owner.accept()) && accepted === 1,
      'Fix acceptance dispatched twice',
    );
    owner.dispose();
    cleanup();
    gate.resolve();
    assert(
      !(await accepting),
      'Disposed fix acceptance authorized a continuation',
    );
    f.dispose();
    passed(
      'Cue fixes acceptance is single-flight while disposal retires owner continuations',
    );
  }
}
async function fixesRecovery() {
  for (const variant of ['clean', 'newer', 'offline', 'disposed']) {
    const initial: DocumentSnapshot = {
      ...emptyDocument,
      loaded: true,
      generation: 1,
      revision: 1,
      cues: [{ id: 1, startMs: 1000, endMs: 3000, text: 'before' }],
    };
    const accepted: DocumentSnapshot = {
      ...initial,
      revision: 2,
      dirty: true,
      canUndo: true,
      cues: [{ ...initial.cues[0]!, text: 'fixed' }],
    };
    const gate = deferred();
    let reads = 0;
    let mutations = 0;
    const session = new DocumentSession({
      documentAction: async (action) => {
        assert(action.kind === 'status', 'Recovery sent a mutation');
        reads++;
        if (reads === 1) return initial;
        await gate.promise;
        if (variant === 'offline') throw new Error('Offline');
        return accepted;
      },
      cueFixesAccept: async () => {
        mutations++;
        throw new Error('Lost reply');
      },
      chooseFile: async () => null,
      gapAccept: async () => initial,
      resyncAccept: async () => initial,
    });
    await session.command({ kind: 'status' });
    session.select(1);
    const result = session.acceptCueFixes(1, initial).catch(() => false);
    await until(() => reads === 2);
    if (variant === 'newer') session.text = 'newer typing';
    if (variant === 'disposed') session.dispose();
    gate.resolve();
    await result;
    assert(mutations === 1, 'Unknown fix acceptance was replayed');
    if (variant === 'disposed') {
      assert(
        session.snapshot === initial,
        'Disposed owner adopted a late refresh',
      );
    } else if (variant === 'offline') {
      assert(
        !session.stateConfirmed && session.editError,
        'Failed refresh was not retained',
      );
      await session.command({ kind: 'undo' }).then(
        () => {
          throw new Error('Mutation allowed without confirmed state');
        },
        () => {},
      );
    } else {
      assert(
        session.snapshot === accepted && session.stateConfirmed,
        'Authoritative fix state missing',
      );
      assert(
        session.text === (variant === 'newer' ? 'newer typing' : 'fixed'),
        'Fix refresh replaced the wrong fields',
      );
    }
    session.dispose();
    passed(`Lost fix reply recovery preserves ${variant} ownership`);
  }
}
async function run() {
  {
    const f = await fixture();
    f.session.text = 'first';
    await pause();
    assert(
      f.requests.length === 1 && !f.session.dirty,
      'Pause did not commit once',
    );
    const identity = f.requests[0]!.session;
    f.session.text = 'second';
    await pause();
    assert(
      identity && f.requests[1]!.session === identity,
      'Focus session changed during typing',
    );
    f.session.text = 'blur';
    f.editing.blur();
    await until(() => !f.session.dirty);
    await pause();
    assert(
      f.requests.length === 3 && f.requests[2]!.session === identity,
      'Blur duplicate or lost group',
    );
    f.editing.focusText();
    f.session.text = 'new focus';
    await pause();
    assert(
      f.requests[3]!.session !== identity,
      'New focus reused grouping identity',
    );
    passed('Pause and blur each commit once, retaining one identity per focus');
    f.dispose();
  }
  {
    const f = await fixture();
    f.editing.composition(true);
    f.session.text = 'composition';
    await pause();
    assert(f.requests.length === 0, 'IME committed incomplete text');
    f.editing.blur();
    await tick();
    assert(f.requests.length === 0, 'Blur committed during IME');
    f.editing.composition(false);
    await until(() => !f.session.dirty);
    await pause();
    assert(
      f.requests.length === 1,
      'IME completion after blur did not flush exactly once',
    );
    passed('IME holds pause and blur until composition completes');
    f.dispose();
  }
  {
    const f = await fixture();
    f.session.text = 'unsent';
    await tick();
    await f.editing.escape();
    await pause();
    assert(
      f.session.text === 'base' && f.requests.length === 0 && f.left() === 1,
      'Escape did not discard unsent draft',
    );
    const gate = f.hold();
    f.session.text = 'sent';
    await pause();
    const escaping = f.editing.escape();
    assert(f.left() === 1, 'Escape left before pending commit');
    gate.resolve();
    await escaping;
    await pause();
    assert(
      f.session.text === 'sent' && f.requests.length === 1 && f.left() === 2,
      'Escape lost confirmed commit',
    );
    passed(
      'Escape cancels unsent text and waits for an already dispatched commit',
    );
    f.dispose();
  }
  {
    const f = await fixture();
    const gate = f.hold();
    f.session.text = 'older';
    await pause();
    f.session.text = 'newer';
    await tick();
    const flushing = f.editing.flush();
    gate.resolve();
    await flushing;
    assert(
      f.requests.map((r) => r.text).join(',') === 'older,newer' &&
        f.session.cue?.text === 'newer',
      'Flush lost or duplicated newer typing',
    );
    await pause();
    assert(f.requests.length === 2, 'Queued timer repeated flushed edit');
    passed(
      'An explicit flush drains newer typing after an older successful reply',
    );
    f.dispose();
  }
  {
    const f = await fixture();
    f.mode('rejected');
    f.session.text = 'retained';
    await pause();
    await pause();
    assert(
      f.requests.length === 1 &&
        f.session.editError &&
        f.session.text === 'retained',
      'Rejection retried or erased text',
    );
    f.mode('success');
    f.session.text = 'corrected';
    await pause();
    assert(
      f.requests.length === 2 && !f.session.dirty && !f.session.editError,
      'New input could not correct rejection',
    );
    passed(
      'Confirmed rejection retains fields and stops retries until new input',
    );
    f.dispose();
  }
  {
    const f = await fixture();
    f.mode('offline');
    f.session.text = 'retained';
    await pause();
    assert(
      !f.session.stateConfirmed && f.requests.length === 1,
      'Unknown outcome not retained',
    );
    await f.editing.escape();
    assert(
      f.left() === 0 && f.session.text === 'retained',
      'Escape discarded an unknown outcome',
    );
    await f.session.command({ kind: 'undo' }).then(
      () => {
        throw new Error('Unconfirmed state allowed a mutation');
      },
      () => {},
    );
    f.mode('success');
    const read = f.holdRead();
    const refresh = f.editing.flush().catch(() => false);
    await until(() => f.reads() === 3);
    await f.session.command({ kind: 'status' }).then(
      () => {
        throw new Error('Overlapping refresh was allowed');
      },
      () => {},
    );
    assert(
      f.reads() === 3,
      'Second request reached the backend during refresh',
    );
    read.resolve();
    await refresh;
    await pause();
    assert(
      f.session.stateConfirmed &&
        f.requests.length === 1 &&
        f.reads() === 3 &&
        f.session.editError,
      'Retry must refresh without replay',
    );
    await f.editing.flush();
    assert(
      f.requests.length === 2 && !f.session.dirty,
      'Reviewed retry failed',
    );
    passed('Unknown outcomes require authoritative refresh and reviewed retry');
    f.dispose();
  }
  {
    const f = await fixture();
    f.session.start = 'invalid';
    f.session.text = 'keep me';
    await pause();
    assert(
      f.requests.length === 0 &&
        f.session.editError &&
        f.session.text === 'keep me',
      'Invalid timing erased input',
    );
    await f.editing.escape();
    assert(
      !f.session.dirty && f.left() === 1,
      'Escape did not restore accepted timing',
    );
    passed('Invalid time stays inline and Escape restores accepted fields');
    f.dispose();
  }
  for (const pending of [false, true]) {
    const f = await fixture();
    const gate = pending ? f.hold() : null;
    f.session.text = 'later';
    await tick();
    if (pending) await pause();
    f.dispose();
    gate?.resolve();
    await pause();
    assert(
      f.requests.length === (pending ? 1 : 0) &&
        f.session.snapshot.revision === 1,
      'Disposed owner dispatched or adopted work',
    );
    passed(`Disposal retires ${pending ? 'pending reply' : 'pause timer'}`);
  }
  await fixesLifecycle();
  await fixesRecovery();
  heading.textContent = `PASS: all ${output.children.length} auto-commit checks`;
}
void run().catch((error) => {
  heading.textContent = `FAIL: ${String(error)}`;
});
