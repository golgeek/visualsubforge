import { DocumentSession } from '../../../src/lib/document-session.svelte';
import {
  documentAction,
  DocumentFailure,
  emptyDocument,
  type DocumentAction,
  type DocumentSnapshot,
} from '../../../src/lib/client';
const output = document.querySelector('#checks')!;
const heading = document.querySelector('#result')!;
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  output.append(row);
}
function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
async function rejected(promise: Promise<unknown>) {
  try {
    await promise;
  } catch {
    return;
  }
  throw new Error('Expected rejection');
}
async function until(check: () => boolean) {
  for (let i = 0; i < 100 && !check(); i++)
    await new Promise((r) => setTimeout(r, 0));
  assert(check(), 'Controlled request not reached');
}
const original: DocumentSnapshot = {
  ...emptyDocument,
  loaded: true,
  generation: 1,
  revision: 1,
  cues: [
    { id: 1, startMs: 1000, endMs: 2000, text: 'base', style: 'Default' },
    { id: 2, startMs: 3000, endMs: 4000, text: 'other' },
  ],
};
const stale = {
  kind: 'staleRevision',
  message: 'A localized message independent of the discriminant',
};
async function fixture(mode: string) {
  let current = structuredClone(original);
  let mutations = 0,
    writes = 0,
    reads = 0;
  let gate: ReturnType<typeof deferred> | null = null;
  let held = '';
  const requests: DocumentAction[] = [];
  Object.defineProperty(window, '__TAURI_INTERNALS__', {
    configurable: true,
    value: {
      invoke: async (
        command: string,
        args: { action: DocumentAction; generation: number; revision: number },
      ) => {
        assert(command === 'document_action', 'Unexpected command');
        const action = args.action;
        requests.push(action);
        if (action.kind === 'status') {
          reads++;
          if (reads > 1 && held === 'refresh') await gate?.promise;
          if (reads > 1 && mode === 'refreshFailure')
            throw new Error('offline');
          return structuredClone(current);
        }
        mutations++;
        if (
          (mutations === 1 && held === 'initial') ||
          (mutations === 2 && held === 'retry')
        )
          await gate?.promise;
        assert(
          action.kind === 'edit' || action.kind === 'editStyled',
          'Unexpected mutation',
        );
        if (action.kind !== 'edit' && action.kind !== 'editStyled')
          throw new Error('not edit');
        if (mutations === 1) {
          if (mode === 'rejected')
            throw { kind: 'rejected', message: 'Invalid text' };
          if (mode === 'lost') {
            current = {
              ...current,
              revision: 2,
              cues: current.cues.map((c) =>
                c.id === 1 ? { ...c, text: action.text } : c,
              ),
            };
            writes++;
            throw new Error('Lost reply');
          }
          current = {
            ...current,
            revision: 2,
            cues: current.cues.map((c) =>
              c.id === 1
                ? { ...c, endMs: 2200 }
                : { ...c, text: 'intervening' },
            ),
          };
          if (mode === 'conflict') current.cues[0]!.text = 'competing';
          if (mode === 'deleted') current.cues = current.cues.slice(1);
          if (mode === 'replacement') current.generation++;
          if (mode === 'unknown')
            throw new Error('Transport disconnected before result');
          throw stale;
        }
        if (mode === 'secondStale') {
          current.revision++;
          throw stale;
        }
        assert(
          args.generation === current.generation &&
            args.revision === current.revision,
          'Retry used stale version',
        );
        current = {
          ...current,
          revision: current.revision + 1,
          canUndo: true,
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
        writes++;
        if (mode === 'lostRetry') throw new Error('Lost retry reply');
        return structuredClone(current);
      },
    },
  });
  const session = new DocumentSession({
    documentAction,
    chooseFile: async () => null,
    gapAccept: async () => current,
    resyncAccept: async () => current,
  });
  await session.command({ kind: 'status' });
  session.select(1);
  session.text = 'my draft';
  return {
    session,
    requests,
    current: () => current,
    mutations: () => mutations,
    writes: () => writes,
    reads: () => reads,
    hold: (phase: string) => {
      held = phase;
      gate = deferred();
      return gate;
    },
  };
}
async function run() {
  {
    const f = await fixture('success');
    assert(await f.session.apply(), 'Retry failed');
    assert(
      f.mutations() === 2 && f.writes() === 1 && f.reads() === 2,
      'Retry not bounded to one accepted write',
    );
    assert(
      f.session.cue?.text === 'my draft' &&
        f.session.cue.endMs === 2200 &&
        f.session.snapshot.cues[1]!.text === 'intervening',
      'Retry overwrote unrelated changes',
    );
    assert(!f.session.dirty, 'Accepted draft not reconciled');
    passed(
      'Structured stale rejection retries once, preserving unrelated same-cue and other-cue fields',
    );
    f.session.dispose();
  }
  for (const mode of [
    'secondStale',
    'conflict',
    'deleted',
    'replacement',
    'lost',
    'unknown',
    'lostRetry',
    'refreshFailure',
  ]) {
    const f = await fixture(mode);
    await rejected(f.session.apply());
    const count = f.mutations();
    assert(
      count === (mode === 'secondStale' || mode === 'lostRetry' ? 2 : 1),
      `${mode}: unsafe retry`,
    );
    assert(
      f.session.text === 'my draft' && f.session.dirty && f.session.conflict,
      `${mode}: draft/conflict lost`,
    );
    await rejected(f.session.apply());
    assert(f.mutations() === count, `${mode}: conflict replayed`);
    assert(
      f.writes() === (mode === 'lost' || mode === 'lostRetry' ? 1 : 0),
      `${mode}: duplicated history`,
    );
    if (mode !== 'refreshFailure')
      assert(
        f.session.snapshot.revision === f.current().revision &&
          f.session.snapshot.generation === f.current().generation,
        `${mode}: authoritative state not reconciled`,
      );
    f.session.revert();
    assert(
      !f.session.conflict && !f.session.dirty,
      `${mode}: explicit discard failed`,
    );
    passed(
      `${mode}: authoritative reconciliation retains draft, blocks replay and permits explicit discard`,
    );
    f.session.dispose();
  }
  {
    const f = await fixture('rejected');
    try {
      await f.session.apply();
    } catch (error) {
      assert(
        error instanceof DocumentFailure &&
          error.detail.kind === 'rejected' &&
          String(error).includes('Invalid text'),
        'Typed error lost message or kind',
      );
    }
    assert(
      f.reads() === 1 && f.mutations() === 1 && f.session.text === 'my draft',
      'Known rejection retried',
    );
    passed(
      'Typed non-version rejection preserves message and draft without retry',
    );
  }
  for (const phase of ['initial', 'refresh', 'retry']) {
    const f = await fixture('success');
    const gate = f.hold(phase);
    const applying = f.session.apply().catch(() => false);
    await until(() =>
      phase === 'initial'
        ? f.mutations() === 1
        : phase === 'refresh'
          ? f.reads() === 2
          : f.mutations() === 2,
    );
    f.session.dispose();
    const count = f.requests.length;
    gate.resolve();
    await applying;
    assert(
      f.requests.length === count &&
        f.session.snapshot.revision === 1 &&
        f.session.text === 'my draft',
      'Disposal continued or adopted edit',
    );
    passed(
      `Disposal during ${phase} suppresses follow-up requests and projection adoption`,
    );
  }
  {
    const f = await fixture('success');
    const gate = f.hold('refresh');
    const applying = f.session.apply();
    await until(() => f.reads() === 2);
    f.session.text = 'newer draft';
    gate.resolve();
    await rejected(applying);
    assert(
      f.mutations() === 1 && f.session.text === 'newer draft',
      'Refresh replayed obsolete draft',
    );
    passed('Draft changed during refresh is preserved and not replayed');
  }
  {
    const f = await fixture('success');
    const gate = f.hold('retry');
    const applying = f.session.apply();
    await until(() => f.mutations() === 2);
    f.session.text = 'newer draft during retry';
    gate.resolve();
    await rejected(applying);
    assert(
      f.session.cue?.text === 'my draft' &&
        f.session.text === 'newer draft during retry' &&
        f.session.dirty,
      'Retry ACK erased newer input',
    );
    passed(
      'Successful retry adopts accepted state without erasing newer draft or authorizing continuation',
    );
  }
  heading.textContent = `PASS: all ${output.children.length} edit retry checks`;
}
void run().catch((error) => {
  heading.textContent = `FAIL: ${String(error)}`;
});
