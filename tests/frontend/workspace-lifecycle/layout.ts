import { WorkspaceLayout } from '../../../src/lib/layout/workspace-layout.svelte';
import { WorkspacePreferences } from '../../../src/lib/settings/workspace-preferences.svelte';
import type { Preferences } from '../../../src/lib/client';

type Layout = Preferences['layout'];
export async function layoutCases(passed: (message: string) => void) {
  const assert = (value: unknown, message: string) => {
    if (!value) throw new Error(message);
  };
  async function until(predicate: () => boolean) {
    const end = performance.now() + 3000;
    while (!predicate()) {
      assert(performance.now() < end, 'Layout lifecycle did not settle');
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }
  const preferences = new WorkspacePreferences();
  preferences.restoreTiming({ ...preferences.timingSettings, cueGapMs: 321 });
  const context = preferences.qualityContext;
  const writes: { value: Layout; resolve: () => void }[] = [];
  const errors: unknown[] = [];
  const owner = new WorkspaceLayout(
    preferences,
    { cue: undefined },
    {
      occlude: async () => {},
      remainingOcclusion: () => false,
      afterRender: async () => {},
      preset: () => {},
      error: (error) => errors.push(error),
    },
    (value) => new Promise((resolve) => writes.push({ value, resolve })),
  );
  try {
    await owner.select('review');
    await until(() => writes.length === 1);
    owner.resizeWaveform(55);
    await until(() => !owner.active);
    await new Promise((resolve) => setTimeout(resolve, 300));
    assert(
      writes.length === 1 && owner.wavePct === 55,
      'Only one save in flight while the newest geometry remains visible',
    );
    writes[0]!.resolve();
    await until(() => writes.length === 2);
    assert(
      owner.wavePct === 55 && writes[1]!.value.review.wavePct === 55,
      'Old save acknowledgment cannot replace newer geometry',
    );
    writes[1]!.resolve();
    await until(() => preferences.preferences.layout.review.wavePct === 55);
    assert(
      preferences.qualityContext === context &&
        preferences.timingSettings.cueGapMs === 321,
      'Layout saves preserve project timing and quality epoch',
    );
    passed(
      'Layout persistence serializes latest sizes without replacing transient geometry or project policy',
    );
    owner.resizeWaveform(50);
    await until(() => !owner.active);
    owner.dispose();
    await new Promise((resolve) => setTimeout(resolve, 300));
    assert(
      writes.length === 2 && errors.length === 0,
      'Disposal cancels unsent persistence',
    );
    passed('Layout disposal retires its save timer');
  } finally {
    owner.dispose();
  }
  for (const remaining of [false, true]) {
    const calls: boolean[] = [];
    const recoveryErrors: unknown[] = [];
    let mutations = 0;
    let renders = 0;
    let nativeOccluded = false;
    const recovering = new WorkspaceLayout(
      preferences,
      { cue: undefined },
      {
        occlude: async (value) => {
          calls.push(value);
          if (calls.length === 2) throw new Error('Rejected native restore');
          nativeOccluded = value;
        },
        remainingOcclusion: () => remaining,
        afterRender: async () => {
          ++renders;
        },
        preset: () => {
          ++mutations;
        },
        error: (error) => recoveryErrors.push(error),
      },
      async () => {},
    );
    try {
      await recovering.select('review');
      assert(
        calls.length === 3 &&
          calls[0] &&
          calls[1] === remaining &&
          calls[2] === remaining,
        'Restore retries once with remaining overlay occlusion',
      );
      assert(
        nativeOccluded === remaining &&
          recoveryErrors.length === 1 &&
          mutations === 1 &&
          renders === 3 &&
          !recovering.active,
        'Recovery reacknowledges bounds without replaying layout or dropping another overlay',
      );
      passed(
        `Layout restore recovery preserves remaining occlusion=${remaining} without replaying mutations`,
      );
    } finally {
      recovering.dispose();
    }
  }
  let releaseRecovery: (() => void) | null = null;
  let recoveryRenders = 0;
  let recoveryCalls = 0;
  const disposedRecovery = new WorkspaceLayout(
    preferences,
    { cue: undefined },
    {
      occlude: async () => {
        ++recoveryCalls;
        if (recoveryCalls === 2)
          throw new Error('Rejected restore before disposal');
      },
      remainingOcclusion: () => false,
      afterRender: async () => {
        if (++recoveryRenders === 3)
          await new Promise<void>((resolve) => {
            releaseRecovery = resolve;
          });
      },
      preset: () => {},
      error: () => {},
    },
    async () => {},
  );
  const recoveringBeforeDisposal = disposedRecovery.select('review');
  await until(() => releaseRecovery !== null);
  disposedRecovery.dispose();
  releaseRecovery!();
  await recoveringBeforeDisposal;
  assert(recoveryCalls === 2, 'Disposed recovery cannot reveal native video');
  passed('Layout disposal retires pending visibility reconciliation');
  const failedCalls: boolean[] = [];
  const repeatedErrors: unknown[] = [];
  const repeated = new WorkspaceLayout(
    preferences,
    { cue: undefined },
    {
      occlude: async (value) => {
        failedCalls.push(value);
        if (!value) throw new Error('Persistent native restore failure');
      },
      remainingOcclusion: () => false,
      afterRender: async () => {},
      preset: () => {},
      error: (error) => repeatedErrors.push(error),
    },
    async () => {},
  );
  await repeated.select('review');
  assert(
    failedCalls.length === 3 && repeatedErrors.length === 2 && !repeated.active,
    'Persistent native failure is reported and bounded to one reconciliation',
  );
  repeated.dispose();
  passed(
    'Repeated layout restore failure has bounded recovery and releases the gate',
  );
  let release!: () => void;
  let presented = false;
  let saved = false;
  const late = new WorkspaceLayout(
    preferences,
    { cue: undefined },
    {
      occlude: () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
      remainingOcclusion: () => false,
      afterRender: async () => {},
      preset: () => {
        presented = true;
      },
      error: (error) => errors.push(error),
    },
    async () => {
      saved = true;
    },
  );
  const changing = late.select('bigPicture');
  late.dispose();
  release();
  await changing;
  assert(
    !presented && !saved && errors.length === 0,
    'Disposed native hide cannot publish a preset or save',
  );
  passed('Layout disposal suppresses late native-hide continuations');
}
