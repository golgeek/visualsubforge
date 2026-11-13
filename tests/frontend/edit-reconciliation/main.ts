import { DocumentSession } from '../../../src/lib/document/document-session.svelte';
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
  } catch (error) {
    return error;
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
    { id: 1, startMs: 1000, endMs: 2000, text: 'base' },
    { id: 2, startMs: 3000, endMs: 4000, text: 'other' },
  ],
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
        if (held === 'initial') await gate?.promise;
        assert(action.kind === 'edit', 'Unexpected mutation');
        if (action.kind !== 'edit') throw new Error('not edit');
        if (mutations === 1 && mode !== 'success' && mode !== 'lost') {
          if (mode === 'rejected')
            throw { kind: 'rejected', message: 'Invalid text' };
          current = {
            ...current,
            revision: 2,
            cues: current.cues.map((c) =>
              c.id === 1 ? { ...c, endMs: 2200 } : c,
            ),
          };
          if (mode === 'conflict') current.cues[0]!.text = 'competing';
          if (mode === 'deleted') current.cues = current.cues.slice(1);
          if (mode === 'replacement') current.generation++;
          if (mode === 'unknown') throw new Error('Transport disconnected');
          if (mode === 'stringFailure') throw 'Transport unavailable';
          if (mode === 'workerUnknown')
            throw { kind: 'outcomeUnknown', message: 'Worker failed' };
          throw {
            kind: mode === 'replacement' ? 'documentReplaced' : 'staleRevision',
            message: 'Localized refusal',
          };
        }
        assert(
          args.generation === current.generation &&
            args.revision === current.revision,
          'Explicit edit used stale version',
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
        if (mode === 'lost') throw new Error('Lost reply');
        return structuredClone(current);
      },
    },
  });
  const session = new DocumentSession({
    documentAction,
    chooseFile: async () => {
      throw new Error('Unexpected chooser');
    },
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
  for (const mode of [
    'stale',
    'conflict',
    'deleted',
    'replacement',
    'lost',
    'unknown',
    'workerUnknown',
    'stringFailure',
    'refreshFailure',
  ]) {
    const f = await fixture(mode);
    const failure = await rejected(f.session.apply());
    const reason =
      mode === 'lost'
        ? 'Lost reply'
        : mode === 'unknown'
          ? 'Transport disconnected'
          : mode === 'workerUnknown'
            ? 'Worker failed'
            : mode === 'stringFailure'
              ? 'Transport unavailable'
              : 'Localized refusal';
    const outcome =
      mode === 'refreshFailure'
        ? 'Could not refresh subtitles; draft retained.'
        : 'Current subtitles refreshed; draft retained. Review it before applying again.';
    assert(
      String(failure) === `Error: ${reason} — ${outcome}`,
      `${mode}: incorrect displayed error: ${String(failure)}`,
    );
    assert(
      failure instanceof Error && failure.cause !== undefined,
      `${mode}: original cause lost`,
    );
    assert(
      f.mutations() === 1 && f.reads() === 2,
      `${mode}: replay or missing refresh`,
    );
    assert(f.session.text === 'my draft', `${mode}: draft lost`);
    assert(
      f.writes() === (mode === 'lost' ? 1 : 0),
      `${mode}: duplicated history`,
    );
    if (mode !== 'refreshFailure')
      assert(
        f.session.snapshot.revision === f.current().revision &&
          f.session.snapshot.generation === f.current().generation,
        `${mode}: state not reconciled`,
      );
    if (mode === 'deleted' || mode === 'replacement') {
      assert(
        f.session.selected === null && f.session.dirty,
        'Draft attached to a different cue',
      );
      assert(
        !(await f.session.save(true)) && f.mutations() === 1,
        'Detached draft authorized save',
      );
    }
    if (mode === 'stale') {
      assert(await f.session.apply(), 'Explicit Apply blocked');
      assert(
        f.mutations() === 2 && f.session.cue?.text === 'my draft',
        'Explicit Apply failed',
      );
    }
    f.session.select(2);
    assert(
      f.session.selected === 2 && f.session.text === 'other',
      `${mode}: selection locked`,
    );
    passed(`${mode}: refresh retains draft without replay or selection lock`);
    f.session.dispose();
  }
  {
    const f = await fixture('rejected');
    const failure = await rejected(f.session.apply());
    assert(
      failure instanceof DocumentFailure &&
        failure.detail.kind === 'rejected' &&
        f.reads() === 1 &&
        f.session.text === 'my draft',
      'Known rejection changed state or message',
    );
    passed(
      'Known rejection preserves typed failure and draft without a refresh',
    );
    f.session.dispose();
  }
  for (const phase of ['initial', 'refresh']) {
    const f = await fixture('stale');
    const gate = f.hold(phase);
    const applying = f.session.apply().catch(() => false);
    await until(() =>
      phase === 'initial' ? f.mutations() === 1 : f.reads() === 2,
    );
    const count = f.requests.length;
    await rejected(f.session.command({ kind: 'undo' }));
    assert(
      f.requests.length === count,
      'Refresh released command serialization',
    );
    f.session.dispose();
    gate.resolve();
    await applying;
    assert(
      f.requests.length === count &&
        f.session.snapshot.revision === 1 &&
        f.session.text === 'my draft',
      'Disposed request continued',
    );
    passed(
      `Disposal during ${phase} suppresses follow-up requests and adoption`,
    );
  }
  for (const phase of ['refresh', 'initial']) {
    const f = await fixture(phase === 'refresh' ? 'stale' : 'success');
    const gate = f.hold(phase);
    const applying = f.session.apply();
    await until(() =>
      phase === 'refresh' ? f.reads() === 2 : f.mutations() === 1,
    );
    f.session.text = 'newer draft';
    gate.resolve();
    await rejected(applying);
    assert(
      f.session.text === 'newer draft' && f.mutations() === 1,
      'New input erased or replayed',
    );
    if (phase === 'initial')
      assert(
        f.session.cue?.text === 'my draft' && f.session.dirty,
        'Successful ACK not adopted',
      );
    passed(`Newer input during ${phase} survives and prevents continuation`);
    f.session.dispose();
  }
  heading.textContent = `PASS: all ${output.children.length} edit reconciliation checks`;
}
void run().catch((error) => {
  heading.textContent = `FAIL: ${String(error)}`;
});
