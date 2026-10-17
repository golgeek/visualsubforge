import {
  QualitySession,
  type QualityInput,
} from '../../../src/lib/quality-session.svelte';
import type { QualityView } from '../../../src/lib/client';

export async function lifecycle(
  sample: QualityView,
  passed: (text: string) => void,
) {
  function assert(value: unknown, message: string) {
    if (!value) throw new Error(message);
  }
  const input: QualityInput = {
    enabled: true,
    generation: sample.key.generation,
    revision: sample.key.revision,
    context: '1',
    selected: 1,
    metricsOffset: 0,
    metricsLimit: 20,
    draft: { cueId: 1, startMs: 1000, endMs: 2000, text: 'Same draft' },
  };
  let release!: (value: QualityView) => void;
  let pending: Promise<QualityView> | null = new Promise((resolve) => {
    release = resolve;
  });
  let result = structuredClone(sample);
  const cancelled: number[] = [];
  let reads = 0;
  let suppliedDrafts = 0;
  const session = new QualitySession({
    readQuality: async (request) => {
      reads++;
      if (request.draft?.kind === 'update') suppliedDrafts++;
      return pending ?? result;
    },
    cancelQuality: async (id) => {
      cancelled.push(id);
    },
  });
  session.configure(input);
  const first = session.poll();
  session.configure({ ...input, revision: input.revision + 1 });
  await session.poll();
  assert(reads === 1, 'Concurrent quality reads were admitted');
  pending = null;
  release(result);
  await first;
  assert(
    session.view === null && session.stale,
    'Old document report replaced current projection',
  );
  result = {
    ...result,
    key: { ...result.key, revision: input.revision + 1 },
    id: sample.id + 1,
  };
  await session.poll();
  assert(
    session.view?.key.revision === input.revision + 1,
    'Current report did not recover',
  );
  assert(
    suppliedDrafts === 2,
    'An obsolete reply suppressed the current draft',
  );
  passed(
    'One in-flight read and revision guards suppress late document reports',
  );
  pending = new Promise((resolve) => {
    release = resolve;
  });
  const last = session.poll();
  session.dispose();
  release({ ...result, id: result.id + 1 });
  await last;
  assert(
    cancelled.includes(result.id + 1),
    'Late worker admission survived disposal',
  );
  const count = reads;
  await session.poll();
  assert(reads === count, 'Disposed session dispatched new work');
  passed('Disposal cancels late admission and prevents further requests');

  let seenDrafts = 0;
  const profile = new QualitySession({
    readQuality: async (request) => {
      if (request.draft) seenDrafts++;
      return result;
    },
    cancelQuality: async () => {},
  });
  profile.configure({
    ...input,
    revision: result.key.revision,
    draft: { cueId: 1, startMs: 1000, endMs: 2000, text: 'Draft' },
  });
  await profile.poll();
  result = { ...result, key: { ...result.key, epoch: result.key.epoch + 1 } };
  await profile.poll();
  await profile.poll();
  assert(
    seenDrafts === 2,
    'Profile invalidation did not resubmit the unchanged local draft',
  );
  profile.dispose();
  passed(
    'Profile epoch changes resubmit draft metrics without changing the draft',
  );
}
