import { expect, test } from '@playwright/test';
import type { QualityRequest } from '../../src/lib/client';

test('a commit reads immediately and waits for the worker silently while draft input remains debounced', async ({
  page,
}) => {
  await page.goto('/tests/frontend/quality-checking/lifecycle.ts');
  await page.clock.install({ time: new Date('2030-01-01T00:00:00Z') });
  await page.clock.pauseAt(new Date('2030-01-01T00:01:00Z'));
  const unit = await page.evaluateHandle(async () => {
    const sessionModule = '/src/lib/quality/quality-session.svelte.ts';
    const sampleModule = '/tests/frontend/ipc-contract-samples.ts';
    const activityModule = '/src/lib/workspace/workspace-activity.svelte.ts';
    const {
      QualitySession,
    }: typeof import('../../src/lib/quality/quality-session.svelte') =
      await import(sessionModule);
    const {
      WorkspaceActivity,
    }: typeof import('../../src/lib/workspace/workspace-activity.svelte') =
      await import(activityModule);
    const { qualityView }: typeof import('./ipc-contract-samples') =
      await import(sampleModule);
    const reads: QualityRequest[] = [];
    const cancels: number[] = [];
    let finish = () => {};
    const activity = new WorkspaceActivity();
    const session = new QualitySession(
      {
        cancelQuality: async (id) => {
          cancels.push(id);
        },
        readQuality: async (request) => {
          reads.push(request);
          if (request.afterWorker != null)
            await new Promise<void>((resolve) => {
              finish = resolve;
            });
          return {
            ...qualityView,
            id: 17,
            key: {
              ...qualityView.key,
              generation: 1,
              revision: request.revision,
            },
            phase:
              request.revision === 2 &&
              request.afterWorker == null &&
              reads.length === 2
                ? 'checking'
                : 'ready',
            draftChecking: false,
          };
        },
      },
      activity,
    );
    const input = {
      generation: 1,
      revision: 1,
      session: 0,
      enabled: true,
      context: 'quality',
      selected: 1,
      metricsOffset: 0,
      metricsLimit: 0,
      draft: null,
    };
    session.configure(input);
    return { session, reads, cancels, activity, input, finish: () => finish() };
  });
  await page.clock.runFor(181);
  expect(await unit.evaluate((u) => u.reads.length)).toBe(1);
  await unit.evaluate((u) => u.session.configure({ ...u.input, revision: 2 }));
  await page.clock.runFor(5);
  expect(
    await unit.evaluate((u) =>
      u.reads.map((r) => [r.revision, r.afterWorker ?? null]),
    ),
  ).toEqual([
    [1, null],
    [2, null],
    [2, 17],
  ]);
  await page.clock.runFor(500);
  expect(
    await unit.evaluate((u) => [u.reads.length, u.activity.visible]),
  ).toEqual([3, false]);
  await unit.evaluate((u) => u.finish());
  await page.clock.runFor(1);
  expect(await unit.evaluate((u) => u.session.view?.phase)).toBe('ready');
  await unit.evaluate((u) =>
    u.session.configure({
      ...u.input,
      revision: 2,
      draft: { cueId: 1, text: 'draft', startMs: 0, endMs: 1000 },
    }),
  );
  await page.clock.runFor(179);
  expect(await unit.evaluate((u) => u.reads.length)).toBe(3);
  await page.clock.runFor(2);
  expect(await unit.evaluate((u) => u.reads.at(-1)?.draft?.kind)).toBe(
    'update',
  );
  const beforePause = await unit.evaluate((u) => {
    u.session.pause();
    u.session.configure({ ...u.input, revision: 3 });
    return u.reads.length;
  });
  await page.clock.runFor(2000);
  expect(await unit.evaluate((u) => u.cancels)).toEqual([17]);
  expect(await unit.evaluate((u) => u.reads.length)).toBe(beforePause);
  await unit.evaluate((u) => u.session.restart());
  await page.clock.runFor(5);
  expect(await unit.evaluate((u) => u.reads.at(-1)?.restart)).toBe(true);
  await unit.evaluate((u) => {
    u.session.dispose();
    u.activity.dispose();
  });
});
