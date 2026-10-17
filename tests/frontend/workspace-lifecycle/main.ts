import { defaultTiming } from '../../../src/lib/magnetic-timing';
import { DocumentSession } from '../../../src/lib/document-session.svelte';
import { MediaSession } from '../../../src/lib/media-session.svelte';
import { ProjectWorkflow } from '../../../src/lib/project-workflow.svelte';
import {
  emptyDocument,
  emptyStatus,
  type DocumentAction,
  type DocumentSnapshot,
  type MediaStatus,
  type OpenedProject,
  type ProjectOptions,
} from '../../../src/lib/client';

const heading = document.getElementById('result')!;
const checks = document.getElementById('checks')!;
function assert(condition: unknown, message: string): void {
  if (!condition) throw new Error(message);
}
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  checks.append(row);
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
async function rejects(promise: Promise<unknown>, message: string) {
  let rejected = false;
  try {
    await promise;
  } catch {
    rejected = true;
  }
  assert(rejected, message);
}
async function until(condition: () => boolean) {
  const deadline = performance.now() + 2000;
  while (!condition()) {
    assert(
      performance.now() < deadline,
      'Timed out waiting for controlled request',
    );
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
  }
}
const cue = (id: number, text = `cue ${id}`) => ({
  id,
  startMs: id * 1000,
  endMs: id * 1000 + 900,
  text,
});
function snapshot(cues = [cue(1), cue(2), cue(3)]): DocumentSnapshot {
  return {
    ...emptyDocument,
    loaded: true,
    generation: 1,
    revision: 2,
    path: '/one.srt',
    cues,
  };
}
function status(session = 1): MediaStatus {
  return {
    ...emptyStatus,
    available: true,
    loaded: true,
    session,
    durationSeconds: 30,
    positionSeconds: 0,
  };
}

async function documentCases() {
  let next = snapshot();
  const requests: {
    action: DocumentAction;
    generation: number;
    revision: number;
  }[] = [];
  let failure = false;
  let pending: ReturnType<typeof deferred<DocumentSnapshot>> | null = null;
  let picker: ReturnType<typeof deferred<string | null>> | null = null;
  let choosing = false;
  const model = new DocumentSession({
    gapAccept: async () => {
      throw new Error('Gap acceptance is not used in this fixture');
    },
    resyncAccept: async () => {
      throw new Error('Resync is not used in this fixture');
    },
    documentAction: async (action, version) => {
      requests.push({ action, ...version });
      if (failure) throw new Error('Rejected edit');
      return pending ? pending.promise : next;
    },
    chooseFile: async () => {
      choosing = true;
      return picker ? picker.promise : null;
    },
  });
  await model.command({ kind: 'status' });
  model.select(2);
  model.text = 'unapplied';
  assert(
    model.snapshot === next &&
      model.snapshot.cues[1]?.text === 'cue 2' &&
      model.dirty,
    'A draft mutated accepted state',
  );
  passed('accepted projection and draft have separate ownership');
  failure = true;
  await rejects(model.apply(), 'Rejected edit appeared accepted');
  assert(
    model.snapshot === next &&
      model.selected === 2 &&
      model.text === 'unapplied',
    'Rejected edit lost the draft',
  );
  failure = false;
  passed('failed edit preserves accepted state, selection and draft');

  pending = deferred<DocumentSnapshot>();
  const apply = model.apply();
  const count = requests.length;
  await rejects(
    model.command({ kind: 'undo' }),
    'Concurrent mutation was allowed',
  );
  assert(requests.length === count, 'Concurrent mutation reached IPC');
  next = { ...snapshot([cue(1), cue(2, 'accepted'), cue(3)]), revision: 3 };
  pending.resolve(next);
  await apply;
  pending = null;
  assert(
    model.snapshot === next && !model.dirty && model.cue?.text === 'accepted',
    'ACK was not reconciled',
  );
  passed('only one document command is in flight and its ACK resets the draft');

  await model.command(
    { kind: 'undo' },
    { version: { generation: 1, revision: 1 } },
  );
  assert(
    requests.at(-1)?.revision === 1,
    'Captured gesture version was replaced by current revision',
  );
  passed('gesture commits retain their captured Rust version');
  next = snapshot([cue(1), cue(3)]);
  await model.command({ kind: 'delete', cueId: 2 });
  assert(
    model.selected === 3 && model.cue?.id === 3,
    'Deleted selection did not reconcile',
  );
  next = { ...snapshot([cue(8)]), generation: 2 };
  await model.command(
    { kind: 'open', path: '/two.srt', discard: true },
    { preferred: 3 },
  );
  assert(
    Number(model.selected) === 8 && model.cue?.id === 8,
    'New generation retained the old selection',
  );
  passed(
    'selection reconciles deleted cues and resets across document generations',
  );

  pending = deferred<DocumentSnapshot>();
  const late = model.command({ kind: 'status' });
  model.dispose();
  pending.resolve(snapshot());
  assert(
    !(await late) && model.snapshot === next,
    'Disposed owner accepted a late response',
  );
  assert(
    !(await model.command({ kind: 'undo' })),
    'Disposed owner issued another command',
  );
  passed('document disposal rejects late projection updates and new commands');

  const saveModel = new DocumentSession({
    gapAccept: async () => {
      throw new Error('Gap acceptance is not used in this fixture');
    },
    resyncAccept: async () => {
      throw new Error('Resync is not used in this fixture');
    },
    documentAction: async (action) => {
      requests.push({ action, generation: 0, revision: 0 });
      return snapshot();
    },
    chooseFile: async () => {
      choosing = true;
      return picker!.promise;
    },
  });
  await saveModel.command({ kind: 'status' });
  picker = deferred<string | null>();
  choosing = false;
  const save = saveModel.save(true);
  await until(() => choosing);
  const beforeSave = requests.length;
  saveModel.dispose();
  picker.resolve('/copy.srt');
  assert(
    !(await save) && requests.length === beforeSave,
    'Late chooser dispatched a save',
  );
  passed('disposing during Save As prevents the subsequent write');
}

async function mediaCases() {
  const errors: unknown[] = [];
  let poll = deferred<MediaStatus>();
  let delayed: ReturnType<typeof deferred<MediaStatus>> | null = null;
  const sent: string[] = [];
  const model = new MediaSession(
    (error) => errors.push(error),
    async (action) => {
      sent.push(action.kind);
      return action.kind === 'status'
        ? poll.promise
        : delayed
          ? delayed.promise
          : status(2);
    },
  );
  const stale = model.poll();
  await model.command({ kind: 'open', path: '/new.mkv', hardware: false });
  poll.resolve(status(1));
  await stale;
  assert(model.status.session === 2, 'Old poll replaced the opened media');
  passed('a status response preceding replacement cannot restore old media');
  poll = deferred<MediaStatus>();
  const staleError = model.poll();
  await model.command({ kind: 'seek', seconds: 2 });
  poll.reject(new Error('old poll failure'));
  await staleError;
  assert(errors.length === 0, 'Old poll error surfaced after a command');
  passed('obsolete media poll errors are suppressed');

  delayed = deferred<MediaStatus>();
  const old = delayed;
  const first = model.command({ kind: 'seek', seconds: 1 });
  await until(() => sent.filter((kind) => kind === 'seek').length === 2);
  delayed = null;
  await model.command({ kind: 'seek', seconds: 2 });
  old.resolve({ ...status(2), positionSeconds: 1 });
  assert(
    !(await first) && model.status.positionSeconds === 0 && !model.pending,
    'Old command response won',
  );
  passed(
    'reordered media command replies cannot overwrite the newest projection',
  );

  model.loop.activate(2);
  delayed = deferred<MediaStatus>();
  const loop = delayed;
  model.loop.update(2, [1, 2]);
  const before = sent.length;
  const opening = model.command({
    kind: 'open',
    path: '/later.mkv',
    hardware: false,
  });
  model.dispose();
  loop.resolve(status(2));
  assert(
    !(await opening) && sent.length === before,
    'Dispose while draining loop sent an open',
  );
  passed(
    'media disposal drains an in-flight loop without dispatching pending replacement',
  );

  let polls = 0;
  const polling = new MediaSession(
    () => {},
    async () => {
      ++polls;
      return status();
    },
  );
  polling.startPolling();
  polling.startPolling();
  await until(() => polls > 0);
  polling.dispose();
  const stopped = polls;
  await new Promise<void>((resolve) => setTimeout(resolve, 130));
  assert(polls === stopped, 'Disposed polling timer remained active');
  await rejects(
    polling.waitUntilLoaded(1),
    'Disposed media authorized readiness',
  );
  passed('polling has one lifetime and disposal cancels readiness');

  let loading = { ...status(1), loaded: false };
  const readiness = new MediaSession(
    () => {},
    async () => loading,
  );
  await readiness.poll();
  const replaced = readiness.waitUntilLoaded(1);
  loading = status(2);
  await readiness.command({
    kind: 'open',
    path: '/replacement.mkv',
    hardware: false,
  });
  await rejects(replaced, 'Replacement authorized a seek in the old session');
  loading = { ...status(2), loaded: false };
  await readiness.poll();
  await rejects(
    readiness.waitUntilLoaded(2, 1),
    'Readiness timeout was ignored',
  );
  const unmounted = readiness.waitUntilLoaded(2);
  readiness.dispose();
  await rejects(unmounted, 'Unmount left a readiness wait live');
  passed(
    'readiness waits reject replacement, expiry and disposal while waiting',
  );
}

const options: ProjectOptions = {
  waveformStart: 0,
  waveformSpan: 20,
  followPlayback: true,
  followSubtitle: true,
  showScenes: true,
};
const project: OpenedProject = {
  token: 7,
  version: 1,
  mediaPath: '/one.mkv',
  subtitlePath: '/one.srt',
  mediaIdentity: { size: 10, modified: '1', sampleHash: 'a' },
  positionSeconds: 12.5,
  options: { ...options, timing: defaultTiming, quality: null },
  scenes: {
    version: 4,
    mode: 'visual',
    cuts: [1],
    identity: { size: 10, modified: '1', sampleHash: 'a' },
  },
};
function projectFixture(hold: string | null = null) {
  const events: string[] = [];
  const gate = deferred<void>();
  let held = false;
  async function step(name: string) {
    events.push(name);
    if (name === hold && !held) {
      held = true;
      await gate.promise;
    }
  }
  let mediaPath = '';
  let current = status();
  const subtitles = new DocumentSession({
    gapAccept: async () => {
      throw new Error('Gap acceptance is not used in this fixture');
    },
    resyncAccept: async () => {
      throw new Error('Resync is not used in this fixture');
    },
    chooseFile: async () => null,
    documentAction: async () => {
      await step('subtitles');
      return snapshot();
    },
  });
  const media = new MediaSession(
    () => {},
    async (action) => {
      await step(`media:${action.kind}`);
      if (action.kind === 'open') current = status(current.session + 1);
      return current;
    },
  );
  const workflow = new ProjectWorkflow(
    subtitles,
    media,
    {
      read: () => ({ mediaPath, hardware: false, options }),
      beginRestore: () => {
        events.push('begin');
        mediaPath = '';
      },
      mediaOpened: (path) => {
        events.push('media projection');
        mediaPath = path;
      },
      restoreOptions: () => {
        events.push('options');
      },
      restoreScenes: () => {
        events.push('scene projection');
      },
      protect: async () => {
        await step('protect');
        return true;
      },
      saveSubtitles: async () => true,
      saveFailed: async () => {
        await step('save choice');
        return 'quit';
      },
      notify: () => {
        events.push('notice');
      },
    },
    {
      chooseFile: async () => {
        await step('chooser');
        return '/one.vsfproject';
      },
      openProject: async () => {
        await step('read');
        return project;
      },
      attachProject: async () => {
        await step('attach');
        return { path: '/one.vsfproject', token: 7 };
      },
      detachProject: async () => {
        await step('detach');
      },
      restoreScenes: async () => {
        await step('scenes');
        return 4;
      },
      saveProject: async () => {
        await step('save');
        return { path: '/copy.vsfproject', token: 8 };
      },
      finishExit: async () => {
        await step('exit');
      },
    },
  );
  return {
    workflow,
    media,
    subtitles,
    events,
    gate,
    dispose() {
      workflow.dispose();
      media.dispose();
      subtitles.dispose();
    },
  };
}
async function projectCases() {
  const normal = projectFixture();
  await normal.workflow.open();
  assert(
    normal.events.join(',') ===
      'chooser,read,protect,begin,media:open,media projection,subtitles,scenes,scene projection,media:seek,attach,options,notice',
    'Project restore ordering changed',
  );
  assert(
    normal.workflow.path === '/one.vsfproject',
    'Completed project not attached',
  );
  await normal.workflow.quit();
  assert(
    normal.events.slice(-4).join(',') === 'protect,save,notice,exit',
    'Quit did not protect, save and exit in order',
  );
  normal.dispose();
  passed(
    'project attachment follows complete restore; quit protects and saves before exit',
  );

  for (const stage of [
    'chooser',
    'read',
    'protect',
    'media:open',
    'subtitles',
    'scenes',
    'media:seek',
    'attach',
  ]) {
    const fixture = projectFixture(stage);
    const opening = fixture.workflow.open().catch(() => {});
    await until(() => fixture.events.includes(stage));
    const before = fixture.events.length;
    fixture.dispose();
    fixture.gate.resolve();
    await opening;
    assert(
      fixture.events.length === before && fixture.workflow.path === null,
      `Disposed ${stage} continued the restore`,
    );
    assert(
      !fixture.events.includes('options'),
      'Unattached project changed workspace settings',
    );
    passed(
      `disposing during project ${stage} suppresses subsequent work and attachment`,
    );
  }
  const saving = projectFixture('save');
  await saving.workflow.open();
  const save = saving.workflow.save(false).catch(() => {});
  await until(() => saving.events.includes('save'));
  await saving.workflow.detach();
  const before = saving.events.length;
  saving.gate.resolve();
  await save;
  assert(
    saving.workflow.path === null && saving.events.length === before,
    'Late save reattached a detached project',
  );
  saving.dispose();
  passed('a late project save cannot reattach after a media detach');

  const replacement = projectFixture('read');
  const earlier = replacement.workflow.open().catch(() => {});
  await until(() => replacement.events.includes('read'));
  await replacement.workflow.open();
  const completed = replacement.events.length;
  replacement.gate.resolve();
  await earlier;
  assert(
    replacement.events.length === completed &&
      replacement.events.filter((event) => event === 'media:open').length ===
        1 &&
      replacement.workflow.path === '/one.vsfproject',
    'Old preflight replaced a later completed open',
  );
  replacement.dispose();
  passed(
    'a superseded project preflight cannot dispatch after a newer open completes',
  );
}

try {
  await documentCases();
  await mediaCases();
  await projectCases();
  heading.textContent = `PASS: all ${checks.children.length} workspace lifecycle checks`;
} catch (error) {
  heading.textContent = `FAIL: ${String(error)}`;
}
