import { StatisticsSession } from '../../../src/lib/statistics-session.svelte';
import { DocumentSession } from '../../../src/lib/document-session.svelte';
import type {
  StatisticsView,
  DocumentSnapshot,
  StatisticsSelection,
} from '../../../src/lib/client';
export async function lifecycle(
  sample: StatisticsView,
  original: DocumentSnapshot,
  passed: (m: string) => void,
) {
  const assert = (v: unknown, m: string) => {
    if (!v) throw new Error(m);
  };
  let snapshot = structuredClone(original);
  const subtitles = new DocumentSession({
    chooseFile: async () => null,
    documentAction: async () => snapshot,
    resyncAccept: async () => snapshot,
    gapAccept: async () => snapshot,
  });
  await subtitles.command({ kind: 'status' });
  let media = 0,
    context = 'scene-a',
    scope = 'all';
  const selection: StatisticsSelection = { range: null, excluded: [] };
  let next = structuredClone(sample);
  let held: Promise<StatisticsView> | null = null;
  let release!: (v: StatisticsView) => void;
  const cancelled: number[] = [];
  let reads = 0;
  const session = new StatisticsSession(
    subtitles,
    () => media,
    () => context,
    () => ({ key: scope, selection }),
    {
      statisticsStart: async () => {
        next = {
          ...next,
          session: media,
          key: {
            ...next.key,
            generation: snapshot.generation,
            revision: snapshot.revision,
          },
        };
        return sample.id;
      },
      statisticsView: async () => {
        reads++;
        return held ?? next;
      },
      statisticsCancel: async (id) => {
        cancelled.push(id);
      },
    },
  );
  await session.start();
  assert(
    session.current && session.view?.phase === 'ready',
    'Initial report unavailable',
  );
  subtitles.text = 'draft typing';
  assert(session.current, 'Draft invalidated accepted statistics');
  await session.poll();
  assert(subtitles.dirty, 'Statistics discarded draft');
  passed(
    'Draft typing leaves accepted statistics current and never acknowledges the draft',
  );
  for (const change of [
    () => {
      scope = 'range';
    },
    () => {
      media = 1;
    },
    () => {
      context = 'scene-b';
    },
    async () => {
      snapshot = { ...snapshot, revision: snapshot.revision + 1 };
      await subtitles.command({ kind: 'status' });
    },
  ]) {
    await session.start();
    held = new Promise((r) => {
      release = r;
    });
    const before = reads;
    const old = session.poll();
    await session.poll();
    assert(reads === before + 1, 'Overlapping polls queued');
    await change();
    assert(!session.current, 'Changed context still current');
    release(next);
    await old;
    held = null;
    assert(
      session.view === null && cancelled.includes(sample.id),
      'Late report survived changed context',
    );
  }
  passed(
    'Scope, media, scene and document changes retire late replies with one in-flight read',
  );
  next = {
    ...next,
    phase: 'failed',
    summary: null,
    findings: [],
    error: 'Injected statistics failure',
  };
  await session.start();
  assert(session.error.includes('Injected'), 'Failure missing');
  next = structuredClone(sample);
  await session.start();
  assert(session.view?.phase === 'ready', 'Failure prevented retry');
  passed('Calculation failure remains read-only and retry recovers');
  held = new Promise((r) => {
    release = r;
  });
  const pending = session.poll();
  session.dispose();
  release(next);
  await pending;
  assert(cancelled.includes(sample.id), 'Dispose did not cancel');
  subtitles.dispose();
  passed('Disposal cancels work and drops late review replies');
}
