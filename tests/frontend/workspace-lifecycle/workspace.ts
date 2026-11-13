import { Workspace } from '../../../src/lib/workspace/workspace.svelte';

export async function workspaceCases(passed: (message: string) => void) {
  const assert = (condition: unknown, message: string) => {
    if (!condition) throw new Error(message);
  };
  let activity = false;
  let confirming = false;
  const workspace = new Workspace({
    activity: () => activity,
    confirming: () => confirming,
    run: async () => {},
  });
  assert(!workspace.canAct, 'Startup must block admission');
  workspace.ready = true;
  assert(workspace.canAct, 'Ready workspace must admit actions');
  activity = true;
  assert(workspace.busy && !workspace.canAct, 'Owner activity must block');
  activity = false;
  confirming = true;
  assert(
    !workspace.busy && !workspace.canAct,
    'Decisions must block admission',
  );
  confirming = false;
  passed('Workspace derives admission from readiness and owner activity');

  let release!: () => void;
  const pending = new Promise<void>((resolve) => (release = resolve));
  workspace.error = 'old error';
  workspace.notice = 'keep notice';
  const task = workspace.task(() => pending);
  assert(
    workspace.busy && workspace.error === '',
    'Task must hold gate and clear error',
  );
  activity = true;
  release();
  assert(await task, 'Successful task must complete');
  assert(workspace.busy, 'Task completion must not release owner activity');
  activity = false;
  assert(
    workspace.canAct && workspace.notice === 'keep notice',
    'Task must preserve notice',
  );
  passed('Workspace task holds its gate independently of other owners');

  const completed = await workspace.task(async () => {
    throw new Error('refused');
  });
  assert(
    !completed && workspace.error.includes('refused') && workspace.canAct,
    'Failure must report and release without continuing',
  );
  await workspace.task(async () => {}, { clearError: false });
  assert(workspace.error.includes('refused'), 'Retained error must survive');
  passed(
    'Workspace task reports failure and preserves explicit retained errors',
  );
  let releaseOuter!: () => void;
  let innerFinished!: () => void;
  const innerDone = new Promise<void>((resolve) => {
    innerFinished = resolve;
  });
  const outerWait = new Promise<void>((resolve) => {
    releaseOuter = resolve;
  });
  const outer = workspace.task(async () => {
    const inner = await workspace.task(async () => {
      throw new Error('inner refused');
    });
    assert(
      !inner && workspace.busy,
      'A nested failure must not release the outer task',
    );
    innerFinished();
    await outerWait;
  });
  await innerDone;
  assert(
    workspace.busy && !workspace.canAct,
    'Outer task must retain admission after the inner settles',
  );
  releaseOuter();
  assert(
    (await outer) && workspace.canAct,
    'Final task completion must release admission',
  );
  passed('Nested task failure preserves the outer workspace gate');

  let finishFirst!: () => void;
  let finishSecond!: () => void;
  const first = workspace.task(
    () =>
      new Promise<void>((resolve) => {
        finishFirst = resolve;
      }),
  );
  const second = workspace.task(
    () =>
      new Promise<void>((resolve) => {
        finishSecond = resolve;
      }),
  );
  finishFirst();
  await first;
  assert(
    workspace.busy,
    'Out-of-order task completion must retain another live task',
  );
  finishSecond();
  await second;
  assert(workspace.canAct, 'All completed tasks must release the gate');
  passed('Concurrent task completion preserves admission until the last task');
}
