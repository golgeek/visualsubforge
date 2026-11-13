import { GapSession } from '../../../src/lib/timing/gap-session.svelte';
import { DocumentSession } from '../../../src/lib/document/document-session.svelte';
import type { GapView, DocumentSnapshot } from '../../../src/lib/client';
export async function lifecycle(
  sample: GapView,
  original: DocumentSnapshot,
  passed: (message: string) => void,
) {
  function assert(v: unknown, m: string) {
    if (!v) throw new Error(m);
  }
  let snapshot = structuredClone(original);
  let release!: (value: DocumentSnapshot) => void;
  let accepting: Promise<DocumentSnapshot> | null = null;
  const subtitles = new DocumentSession({
    chooseFile: async () => null,
    documentAction: async () => snapshot,
    resyncAccept: async () => {
      throw new Error('Unused');
    },
    gapAccept: async () => accepting ?? snapshot,
  });
  await subtitles.command({ kind: 'status' });
  let media = 0;
  let next = structuredClone(sample);
  let readRelease!: (v: GapView) => void;
  let held: Promise<GapView> | null = null;
  let reads = 0;
  const cancelled: number[] = [];
  const session = new GapSession(subtitles, () => media, {
    gapStart: async () => sample.id,
    gapView: async (_id, _offset, limit) => {
      assert(limit === 100, 'Review requested an unbounded page');
      reads++;
      return held ?? next;
    },
    gapCancel: async (id) => {
      cancelled.push(id);
    },
  });
  await session.start();
  session.acknowledgeNoMedia = true;
  assert(session.canAccept, 'Ready proposal not acceptable');
  held = new Promise((resolve) => {
    readRelease = resolve;
  });
  const old = session.poll();
  const priorReads = reads;
  await session.poll();
  await session.poll();
  assert(reads === priorReads, 'Polling queued additional reads');
  passed('Only one background poll can remain in flight');
  media = 1;
  readRelease(next);
  await old;
  held = null;
  assert(
    session.view === null &&
      !session.canAccept &&
      cancelled.includes(sample.id),
    'Late media reply remained acceptable',
  );
  passed('Media replacement during a held review reply retires the proposal');
  media = 0;
  await session.start();
  assert(
    !session.acknowledgeNoMedia && !session.canAccept,
    'A new proposal inherited the previous acknowledgment',
  );
  snapshot = { ...snapshot, revision: snapshot.revision + 1 };
  await subtitles.command({ kind: 'status' });
  await session.poll();
  assert(
    session.view === null && !session.canAccept,
    'New document snapshot retained old proposal',
  );
  passed(
    'Accepted-document replacement invalidates preview even when cue contents match',
  );
  for (const rejectOld of [false, true]) {
    await session.start();
    let reject!: (reason: Error) => void;
    held = new Promise((resolve, fail) => {
      readRelease = resolve;
      reject = fail;
    });
    const obsolete = session.poll();
    held = null;
    next = { ...next, offset: 100 };
    await session.page(100);
    if (rejectOld) reject(new Error('obsolete page error'));
    else readRelease({ ...next, offset: 0 });
    await obsolete;
    assert(
      session.view?.offset === 100 && !session.error,
      'Obsolete page or error replaced current page',
    );
  }
  passed('Paging excludes both obsolete poll results and errors');
  held = new Promise((resolve) => {
    readRelease = resolve;
  });
  session.acknowledgeNoMedia = true;
  assert(session.canAccept, 'Policy control was not initially acceptable');
  const policyPoll = session.poll();
  readRelease({ ...next, phase: 'stale' });
  await policyPoll;
  held = null;
  assert(
    !session.canAccept && session.view?.phase === 'stale',
    'Rust policy invalidation remained acceptable',
  );
  passed('Policy invalidation during a pending read disables acceptance');
  await session.start();
  held = new Promise((resolve) => {
    readRelease = resolve;
  });
  session.acknowledgeNoMedia = true;
  assert(session.canAccept, 'Document control was not initially acceptable');
  const documentPoll = session.poll();
  snapshot = { ...snapshot, revision: snapshot.revision + 1 };
  await subtitles.command({ kind: 'status' });
  readRelease(next);
  await documentPoll;
  held = null;
  assert(
    !session.view && !session.canAccept,
    'Pending document reply remained visible',
  );
  passed('Document replacement during a pending read excludes the old page');
  next = { ...next, key: { ...next.key, revision: snapshot.revision } };
  await session.start();
  session.acknowledgeNoMedia = true;
  accepting = new Promise((resolve) => {
    release = resolve;
  });
  const result = session.accept();
  assert(
    !session.canClose && !session.canStart,
    'Acceptance permits modal close or another request',
  );
  release({ ...snapshot, revision: snapshot.revision + 1, dirty: true });
  assert(await result, 'Accepted reply not adopted');
  assert(
    subtitles.snapshot.revision === snapshot.revision + 1,
    'Document owner did not reconcile accepted reply',
  );
  passed(
    'Modal stays protected during acceptance and DocumentSession alone adopts the reply',
  );
  await session.start();
  held = new Promise((resolve) => {
    readRelease = resolve;
  });
  session.acknowledgeNoMedia = true;
  assert(session.canAccept, 'Disposal control was not initially acceptable');
  const disposedPoll = session.poll();
  const beforeDisposal = session.view;
  const cancellationsBeforeDisposal = cancelled.length;
  session.dispose();
  assert(
    cancelled.length === cancellationsBeforeDisposal + 1,
    'Disposal did not cancel the active proposal',
  );
  readRelease({ ...next, error: 'late disposed error' });
  await disposedPoll;
  assert(
    session.view === beforeDisposal && !session.error && !session.canAccept,
    'Disposed read updated the projection',
  );
  passed('Disposal cancels the job and excludes its pending read');
  subtitles.dispose();
}
