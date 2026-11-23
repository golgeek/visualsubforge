import { TimingFixSession } from '../../../src/lib/timing/timing-fix-session.svelte';
import { defaultPreferences } from '../../../src/lib/client';
import { DocumentSession } from '../../../src/lib/document/document-session.svelte';
import type { TimingFixView, DocumentSnapshot } from '../../../src/lib/client';
export async function lifecycle(
  sample: TimingFixView,
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
    timingFixAccept: async () => accepting ?? snapshot,
  });
  await subtitles.command({ kind: 'status' });
  let media = 0;
  let next = structuredClone(sample);
  let readRelease!: (v: TimingFixView) => void;
  let held: Promise<TimingFixView> | null = null;
  const cancelled: number[] = [];
  let scope: string | null = null;
  const session = new TimingFixSession(
    subtitles,
    () => media,
    () => ({
      options: defaultPreferences.timingRepair,
      families: { gaps: true, speed: true, shots: true },
      scope: scope === null ? null : [Number(scope)],
      toggled: [],
      omitTrades: false,
    }),
    undefined,
    {
      timingFixStart: async () => sample.id,
      timingFixView: async () => held ?? next,
      timingFixCancel: async (id) => {
        cancelled.push(id);
      },
    },
  );
  await session.start();
  assert(session.canAccept, 'Ready proposal not acceptable');
  held = new Promise((resolve) => {
    readRelease = resolve;
  });
  const scopePoll = session.poll();
  scope = '2';
  assert(!session.canAccept, 'Changed scope remains acceptable');
  readRelease(next);
  await scopePoll;
  held = null;
  assert(
    session.view === null && cancelled.includes(sample.id),
    'Changed scope was not cancelled',
  );
  scope = null;
  await session.start();
  passed(
    'Scope changes invalidate both immediate acceptance and later review replies',
  );

  held = new Promise((resolve) => {
    readRelease = resolve;
  });
  const old = session.poll();
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
  next = { ...next, key: { ...next.key, revision: snapshot.revision } };
  await session.start();
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
  session.dispose();
  subtitles.dispose();
}
