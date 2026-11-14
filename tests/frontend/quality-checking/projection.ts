import {
  QualitySession,
  type QualityInput,
} from '../../../src/lib/quality/quality-session.svelte';
import type { QualityView } from '../../../src/lib/client';

export async function projection(
  sample: QualityView,
  passed: (text: string) => void,
) {
  const assert = (value: unknown, message: string) => {
    if (!value) throw new Error(message);
  };
  const input: QualityInput = {
    session: 0,
    enabled: true,
    generation: sample.key.generation,
    revision: sample.key.revision,
    context: 'index',
    selected: null,
    metricsOffset: 0,
    metricsLimit: 200,
    draft: null,
    indexIssues: true,
    metricOffsets: [0, 400],
  };
  const findings: QualityView['findings'] = Array.from(
    { length: 450 },
    (_, i) => ({
      ...sample.findings[0]!,
      cueId: String(i + 1),
      cueIndex: i,
      severity: i === 449 ? 'violation' : 'advisory',
      relatedCueId: i === 449 ? '1' : null,
    }),
  );
  let epoch = sample.key.epoch;
  let release: ((value: QualityView) => void) | null = null;
  let held = false;
  let reads = 0;
  let fail = false;
  const session = new QualitySession({
    readQuality: async (request) => {
      reads++;
      if (fail) throw new Error('Projection transport failed');
      const page: QualityView = {
        ...structuredClone(sample),
        key: { ...sample.key, epoch },
        phase: 'ready',
        offset: request.offset,
        findingsCount: findings.length,
        filteredCount: findings.length,
        findings: findings.slice(
          request.offset,
          request.offset + request.limit,
        ),
        metrics: request.metricsLimit
          ? [
              {
                ...sample.metrics[0]!,
                data: {
                  ...sample.metrics[0]!.data,
                  cueId: String(request.metricsOffset + 1),
                  cueIndex: request.metricsOffset,
                },
              },
            ]
          : [],
      };
      if (held && !request.run)
        return new Promise<QualityView>((resolve) => {
          release = resolve;
        });
      return page;
    },
    cancelQuality: async () => {},
  });
  try {
    session.configure(input);
    await session.poll();
    assert(
      session.issues.size === 300 && !session.issueIndexComplete,
      'Partial issue coverage was misrepresented',
    );
    assert(
      session.view?.findings.length === 100,
      'Index paging replaced Problems page',
    );
    await session.poll();
    assert(
      session.issues.size === 450 && session.issueIndexComplete,
      'Issue index did not complete bounded pages',
    );
    assert(
      session.issues.get('1') === 'violation',
      'Related-cue severity did not override advisory',
    );
    assert(
      session.metrics.has('401'),
      'Visible original-index metric window was not cached',
    );
    passed(
      'Issue pages preserve Problems filters, related-cue severity and original-index metric windows',
    );

    for (let i = 1; i <= 40; i++) {
      session.configure({
        ...input,
        metricsOffset: i * 200,
        metricOffsets: [i * 200],
      });
      await session.poll();
    }
    assert(
      session.metrics.size <= 32 && !session.metrics.has('1'),
      'Metric windows grew without eviction',
    );
    passed(
      'Metric cache evicts old windows instead of retaining the entire document',
    );

    epoch++;
    held = true;
    session.configure(input);
    const pending = session.poll();
    await Promise.resolve();
    await Promise.resolve();
    assert(release, 'Expected a held secondary page');
    const before = reads;
    await session.poll();
    assert(reads === before, 'Secondary paging allowed a concurrent poll');
    session.configure({ ...input, context: 'replacement' });
    const finish = release as unknown as (view: QualityView) => void;
    finish({
      ...sample,
      key: { ...sample.key, epoch },
      phase: 'ready',
      offset: 100,
      findings: findings.slice(100, 300),
      findingsCount: 450,
    });
    await pending;
    assert(
      session.issues.size === 0 && !session.issueIndexComplete,
      'Retired page republished issue markers',
    );
    held = false;
    await session.poll();
    fail = true;
    await session.poll();
    assert(
      session.issues.size === 0 &&
        session.metrics.size === 0 &&
        !!session.error,
      'Failed read retained current-looking markers',
    );
    passed(
      'Secondary pages stay single-flight; replacement and failed reads retire visible markers',
    );
  } finally {
    session.dispose();
  }
}
