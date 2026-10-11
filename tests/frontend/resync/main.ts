import { mount, unmount, tick } from 'svelte';
import { isTauri } from '@tauri-apps/api/core';
import App from '../../../src/App.svelte';
import { DocumentSession } from '../../../src/lib/document-session.svelte';
import { ResyncSession } from '../../../src/lib/resync-session.svelte';
import {
  emptyDocument,
  emptyStatus,
  defaultPreferences,
  type DocumentSnapshot,
  type ResyncView,
  type DocumentAction,
  type MediaAction,
  type updateMenu,
} from '../../../src/lib/client';

if (isTauri()) throw new Error('Browser-only modeled IPC fixture');
const host = document.getElementById('test')!;
const heading = document.getElementById('result')!;
const output = document.getElementById('checks')!;
function assert(value: unknown, message: string): void {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  output.append(row);
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
async function until(check: () => boolean) {
  await tick();
  const deadline = performance.now() + 5000;
  while (!check()) {
    assert(
      performance.now() < deadline,
      'Timed out waiting for UI: ' + host.textContent,
    );
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await tick();
}
function snapshot(count = 2): DocumentSnapshot {
  return {
    ...emptyDocument,
    generation: 1,
    loaded: true,
    path: '/original.srt',
    cues: Array.from({ length: count }, (_, i) => ({
      id: i + 1,
      startMs: 1000 + i * 2000,
      endMs: 2000 + i * 2000,
      text: `Cue ${i + 1}`,
    })),
  };
}
function view(
  doc = snapshot(),
  phase: ResyncView['phase'] = 'reviewing',
  offset = 0,
): ResyncView {
  return {
    id: 1,
    phase,
    stage: 'Matching content',
    error: null,
    complete: true,
    totalCues: doc.cues.length,
    unresolvedCues: 0,
    totalSections: 1,
    cueOffset: offset,
    sectionOffset: 0,
    cues: doc.cues.slice(offset, offset + 100).map((cue) => ({
      id: String(cue.id),
      text: cue.text,
      sourceStartMs: cue.startMs,
      sourceEndMs: cue.endMs,
      startMs: cue.startMs + 500,
      endMs: cue.endMs + 500,
      issue: null,
    })),
    sections: [
      {
        sourceStartUs: 0,
        sourceEndUs: 500000000,
        targetStartUs: 500000,
        targetEndUs: 500500000,
        anchors: 12,
        maxResidualUs: 0,
      },
    ],
  };
}
async function fixture() {
  let doc = snapshot();
  const state = {
    media: { ...emptyStatus, loaded: true, session: 1 },
    starts: 0,
    accepts: 0,
    cancelled: [] as number[],
    captured: null as Pick<DocumentSnapshot, 'generation' | 'revision'> | null,
  };
  const docApi = {
    chooseFile: async () => null as string | null,
    documentAction: async (action: DocumentAction) => {
      if (action.kind === 'edit')
        doc = {
          ...doc,
          revision: doc.revision + 1,
          dirty: true,
          cues: doc.cues.map((c) =>
            c.id === action.cueId ? { ...c, text: action.text } : c,
          ),
        };
      if (action.kind === 'save' && action.path)
        doc = { ...doc, path: action.path };
      return doc;
    },
    resyncAccept: async () => {
      state.accepts++;
      return {
        ...doc,
        revision: doc.revision + 1,
        dirty: true,
        requiresSaveAs: true,
        canUndo: true,
        cues: doc.cues.map((cue) => ({
          ...cue,
          startMs: cue.startMs + 500,
          endMs: cue.endMs + 500,
        })),
      };
    },
  };
  const subtitles = new DocumentSession(docApi);
  await subtitles.command({ kind: 'status' });
  let phase: ResyncView['phase'] = 'reviewing';
  const api = {
    chooseFile: async () => '/source.mkv' as string | null,
    resyncStart: async (
      _source: string,
      _session: number,
      document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
    ) => {
      state.starts++;
      state.captured = document;
      return 1;
    },
    resyncView: async (_id: number, cueOffset = 0) =>
      view(doc, phase, cueOffset),
    resyncCancel: async (id: number) => {
      state.cancelled.push(id);
      phase = 'cancelled';
    },
  };
  const session = new ResyncSession(subtitles, () => state.media, api);
  session.source = '/source.mkv';
  return {
    subtitles,
    session,
    api,
    docApi,
    state,
    dispose: () => {
      session.dispose();
      subtitles.dispose();
    },
  };
}

async function ownerCases() {
  {
    const f = await fixture();
    f.subtitles.text = 'Unapplied';
    await f.session.start();
    assert(
      f.state.starts === 0 && f.subtitles.dirty,
      'Analysis bypassed the draft decision',
    );
    await f.session.applyDraft();
    await f.session.start();
    assert(
      f.state.captured?.revision === 1 &&
        f.subtitles.snapshot.cues[0]?.text === 'Unapplied' &&
        f.session.canAccept,
      'Analysis did not capture the applied draft',
    );
    f.dispose();
    passed(
      'analysis requires draft resolution and captures the accepted revision',
    );
  }
  {
    const f = await fixture();
    f.api.resyncView = async () => ({
      ...view(),
      complete: false,
      unresolvedCues: 1,
    });
    await f.session.start();
    assert(
      !(await f.session.accept()) && f.state.accepts === 0,
      'Incomplete proposal reached acceptance',
    );
    f.dispose();
    passed('incomplete proposals cannot invoke acceptance');
  }
  {
    const f = await fixture();
    await f.session.start();
    await f.subtitles.command({ kind: 'save', path: '/copy.srt' });
    assert(
      !f.session.canAccept,
      'Save As with unchanged revision left acceptance enabled',
    );
    await f.session.poll();
    assert(
      f.state.cancelled.includes(1) && f.session.view === null,
      'Stale review was not retired after Save As',
    );
    f.dispose();
    passed(
      'Save As invalidates a reviewed proposal even without an editor revision',
    );
  }
  for (const change of ['dispose', 'media', 'document'] as const) {
    const f = await fixture();
    const pending = deferred<number>();
    f.api.resyncStart = async () => pending.promise;
    const start = f.session.start();
    if (change === 'dispose') f.session.dispose();
    if (change === 'media') f.state.media = { ...f.state.media, session: 2 };
    if (change === 'document')
      await f.subtitles.command({
        kind: 'edit',
        cueId: 1,
        startMs: 1000,
        endMs: 2000,
        text: 'Changed',
      });
    pending.resolve(21);
    await start;
    assert(
      f.state.cancelled.includes(21) &&
        !f.session.canAccept &&
        f.session.view === null,
      'Late start leaked or adopted a job: ' + change,
    );
    f.dispose();
    passed('late start is cancelled after ' + change);
  }
  {
    const f = await fixture();
    const picker = deferred<string | null>();
    f.api.chooseFile = async () => picker.promise;
    const choose = f.session.chooseSource();
    f.session.dispose();
    picker.resolve('/late.mkv');
    await choose;
    assert(
      f.session.source === '/source.mkv' && f.state.starts === 0,
      'Disposed chooser changed input',
    );
    f.dispose();
    passed('late source picker cannot act after dialog disposal');
  }
  for (const failure of [false, true]) {
    const f = await fixture();
    f.api.resyncView = async () => view(snapshot(), 'analyzing');
    await f.session.start();
    const response = deferred<ResyncView>();
    f.api.resyncView = async () => response.promise;
    const poll = f.session.poll();
    f.api.resyncView = async () => view(snapshot(), 'cancelled');
    await f.session.cancel();
    if (failure) response.reject(new Error('Obsolete failure'));
    else response.resolve(view());
    await poll;
    assert(
      f.session.view?.phase === 'cancelled' &&
        !f.session.canAccept &&
        !f.session.error,
      'Cancelled job adopted late response',
    );
    f.dispose();
    passed(
      'cancel rejects an older ' + (failure ? 'error' : 'review response'),
    );
  }
  {
    const f = await fixture();
    await f.session.start();
    const response = deferred<ResyncView>();
    f.api.resyncView = async () => response.promise;
    const page = f.session.page(100, 0);
    f.session.dispose();
    response.resolve(view(snapshot(105), 'reviewing', 100));
    await page;
    assert(f.session.view?.cueOffset === 0, 'Disposed page changed projection');
    f.dispose();
    passed('late page response cannot replace a disposed review');
  }
  {
    const f = await fixture();
    await f.session.start();
    f.api.chooseFile = async () => '/other-source.mkv';
    await f.session.chooseSource();
    assert(
      f.state.cancelled.includes(1) &&
        f.session.view === null &&
        !f.session.canAccept,
      'Source replacement retained acceptance',
    );
    f.dispose();
    passed('source changes retire the previous proposal');
  }
  {
    const f = await fixture();
    await f.session.start();
    const response = deferred<DocumentSnapshot>();
    f.docApi.resyncAccept = async () => {
      f.state.accepts++;
      return response.promise;
    };
    const accepting = f.session.accept();
    assert(!(await f.session.accept()), 'Duplicate acceptance started');
    let rejected = false;
    try {
      await f.subtitles.command({ kind: 'undo' });
    } catch {
      rejected = true;
    }
    assert(rejected, 'Document command raced acceptance');
    await f.session.cancel();
    const accepted = {
      ...snapshot(),
      revision: 1,
      dirty: true,
      requiresSaveAs: true,
      canUndo: true,
    };
    response.resolve(accepted);
    assert(
      await accepting,
      'Acknowledged commit was discarded after late cancel',
    );
    assert(
      f.subtitles.snapshot === accepted && f.state.accepts === 1,
      'Document owner did not adopt exactly one acceptance',
    );
    f.dispose();
    passed(
      'acceptance serializes document commands and adopts a commit preceding cancellation',
    );
  }
  {
    const f = await fixture();
    await f.session.start();
    const before = f.subtitles.snapshot;
    f.docApi.resyncAccept = async () => {
      throw new Error('Input changed');
    };
    assert(
      !(await f.session.accept()) &&
        f.subtitles.snapshot === before &&
        f.session.error.includes('Input changed'),
      'Failed acceptance mutated the document',
    );
    f.dispose();
    passed(
      'acceptance failure preserves the accepted projection and reports the cause',
    );
  }
  {
    const f = await fixture();
    await f.session.start();
    const before = f.subtitles.snapshot;
    const response = deferred<DocumentSnapshot>();
    f.docApi.resyncAccept = async () => response.promise;
    const accepting = f.session.accept();
    f.dispose();
    response.resolve({ ...snapshot(), revision: 1 });
    assert(
      !(await accepting) && f.subtitles.snapshot === before,
      'Disposed workspace adopted late acceptance',
    );
    passed(
      'workspace disposal suppresses late acceptance projection and follow-up',
    );
  }
}

async function appCases() {
  let doc = snapshot(205);
  const original = JSON.stringify(doc.cues);
  let media = {
    ...emptyStatus,
    available: true,
    loaded: true,
    session: 1,
    durationSeconds: 500,
    positionSeconds: 0,
    paused: true,
    surfaceAboveWebview: true,
  };
  let occluded = false;
  let phase: ResyncView['phase'] = 'reviewing';
  let incomplete = false;
  let id = 0;
  let captured = structuredClone(doc);
  let before: DocumentSnapshot | null = null;
  let after: DocumentSnapshot | null = null;
  let savePath: string | null = '/target.srt';
  let failSave = false;
  let editGate: Promise<void> | null = null;
  const files = new Map([['/original.srt', original]]);
  const calls: string[] = [];
  let projectSubtitle: unknown;
  const callbacks = new Map<number, (value: unknown) => void>();
  let listener = 0;
  let menu: Parameters<typeof updateMenu>[0] | undefined;
  Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
  Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
    configurable: true,
    value: { unregisterListener: () => {} },
  });
  Object.defineProperty(window, '__TAURI_INTERNALS__', {
    configurable: true,
    value: {
      transformCallback: (callback: (value: unknown) => void) => {
        const id = callbacks.size + 1;
        callbacks.set(id, callback);
        return id;
      },
      unregisterCallback: () => {},
      async invoke(
        command: string,
        args: {
          action?: DocumentAction | MediaAction;
          kind?: string;
          handler?: number;
          state?: Parameters<typeof updateMenu>[0];
          cueOffset?: number;
          request?: Record<string, unknown>;
        },
      ) {
        calls.push(command);
        switch (command) {
          case 'preferences_get':
            return { ...defaultPreferences, autoScanScenes: false };
          case 'plugin:event|listen':
            listener = args.handler!;
            return 1;
          case 'plugin:event|unlisten':
            return null;
          case 'update_menu':
            menu = args.state;
            return null;
          case 'recovery_status':
            return { entries: [], error: null };
          case 'choose_file':
            return args.kind === 'media'
              ? host.querySelector('dialog[open]')
                ? '/source.mkv'
                : '/target.mkv'
              : args.kind === 'saveProject'
                ? '/target.vsfproject'
                : savePath;
          case 'project_detach':
            return null;
          case 'project_save':
            assert(
              !doc.requiresSaveAs,
              'Project saved before protected subtitles',
            );
            projectSubtitle = args.request!.subtitlePath;
            return { path: '/target.vsfproject', token: 1 };
          case 'media_action': {
            const action = args.action as MediaAction;
            if (action.kind === 'occlude') occluded = action.occluded;
            if (action.kind === 'open')
              media = { ...media, session: media.session + 1 };
            return structuredClone(media);
          }
          case 'resync_start':
            assert(occluded, 'Analysis started without modal occlusion');
            captured = structuredClone(doc);
            return ++id;
          case 'resync_view':
            return {
              ...view(captured, phase, args.cueOffset),
              id,
              complete: !incomplete,
              unresolvedCues: incomplete ? 1 : 0,
              cues: view(captured, phase, args.cueOffset).cues.map((cue, i) =>
                incomplete && i === 0
                  ? {
                      ...cue,
                      startMs: null,
                      endMs: null,
                      issue: 'No reliable matching content',
                    }
                  : cue,
              ),
            };
          case 'resync_cancel':
            phase = 'cancelled';
            return null;
          case 'resync_accept': {
            assert(!incomplete, 'Incomplete accept reached transport');
            before = structuredClone(doc);
            doc = {
              ...doc,
              revision: doc.revision + 1,
              dirty: true,
              canUndo: true,
              canRedo: false,
              requiresSaveAs: true,
              cues: doc.cues.map((cue) => ({
                ...cue,
                startMs: cue.startMs + 500,
                endMs: cue.endMs + 500,
              })),
            };
            after = structuredClone(doc);
            phase = 'accepted';
            return structuredClone(doc);
          }
          case 'document_action': {
            const action = args.action as DocumentAction;
            if (action.kind === 'edit' && editGate) await editGate;
            if (action.kind === 'edit')
              doc = {
                ...doc,
                revision: doc.revision + 1,
                dirty: true,
                cues: doc.cues.map((cue) =>
                  cue.id === action.cueId
                    ? {
                        ...cue,
                        text: action.text,
                        startMs: action.startMs,
                        endMs: action.endMs,
                      }
                    : cue,
                ),
              };
            if (action.kind === 'undo')
              doc = {
                ...doc,
                revision: doc.revision + 1,
                cues: structuredClone(before!.cues),
                canUndo: false,
                canRedo: true,
              };
            if (action.kind === 'redo')
              doc = {
                ...doc,
                revision: doc.revision + 1,
                cues: structuredClone(after!.cues),
                canUndo: true,
                canRedo: false,
              };
            if (action.kind === 'save') {
              if (failSave) throw 'Injected save failure';
              if (!action.path || action.path === '/original.srt')
                throw 'Choose a different file to preserve the original subtitles';
              files.set(action.path, JSON.stringify(doc.cues));
              doc = {
                ...doc,
                path: action.path,
                requiresSaveAs: false,
                dirty: false,
              };
            }
            return structuredClone(doc);
          }
          case 'workflow_trace_enabled':
            return false;
          default:
            throw new Error('Unexpected IPC: ' + command);
        }
      },
    },
  });
  const app = mount(App, { target: host });
  const dispatch = (action: string) =>
    callbacks.get(listener)!({ payload: action });
  function button(label: string, root: ParentNode = host) {
    const button = [...root.querySelectorAll('button')].find(
      (b) => b.textContent?.trim() === label,
    );
    assert(button, 'Missing button: ' + label);
    return button!;
  }
  const dialog = () =>
    host.querySelector<HTMLDialogElement>(
      'dialog[aria-labelledby="resync-title"][open]',
    );
  async function open() {
    phase = 'reviewing';
    dispatch('resync');
    await until(() => !!dialog());
    assert(occluded, 'Dialog visible before native occlusion');
  }
  async function analyze() {
    button('Choose source video…', dialog()!).click();
    await until(() => !button('Analyze videos', dialog()!).disabled);
    button('Analyze videos', dialog()!).click();
    await until(() => !!dialog()?.querySelector('table'));
  }
  try {
    await until(() => !!listener && !!menu && !menu.busy);
    dispatch('openMedia');
    await until(() => !!menu && !menu.busy && media.session === 2);
    if (new URLSearchParams(location.search).has('review')) {
      await open();
      await analyze();
      heading.textContent = 'Interactive resync review (modeled IPC)';
      // Manual layout/keyboard inspection only; never reports automated PASS.
      await new Promise<void>(() => {});
    }
    const text = host.querySelector<HTMLTextAreaElement>('textarea')!;
    text.value = 'Draft revised';
    text.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    await open();
    assert(
      dialog()!.textContent?.includes('unapplied draft'),
      'Draft decision missing',
    );
    assert(
      button('Analyze videos', dialog()!).disabled,
      'Draft analysis was enabled',
    );
    button('Cancel resync', dialog()!).click();
    await until(() => !dialog() && !menu!.busy);
    assert(
      text.value === 'Draft revised' && doc.cues[0]?.text === 'Cue 1',
      'Cancelling resync lost or committed the draft',
    );
    passed(
      'App requires a draft decision and Cancel retains the unapplied draft',
    );
    await open();
    button('Discard cue draft', dialog()!).click();
    await tick();
    assert(
      text.value === 'Cue 1' && doc.cues[0]?.text === 'Cue 1',
      'Discarding a draft changed accepted content',
    );
    button('Close', dialog()!).click();
    await until(() => !dialog() && !menu!.busy);
    text.value = 'Draft revised';
    text.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    passed(
      'Discard cue draft restores accepted content without a document command',
    );
    await open();
    const draftReply = deferred<void>();
    editGate = draftReply.promise;
    button('Apply cue draft', dialog()!).click();
    await until(() => button('Close', dialog()!).disabled);
    assert(
      button('Cancel resync', dialog()!).disabled,
      'Draft commit allowed modal close before acknowledgment',
    );
    draftReply.resolve();
    editGate = null;
    await until(() => !dialog()!.textContent?.includes('unapplied draft'));
    incomplete = true;
    await analyze();
    assert(
      doc.cues[0]?.text === 'Draft revised' &&
        button('Accept timings', dialog()!).disabled &&
        dialog()!.textContent?.includes('No reliable matching content'),
      'Incomplete review or applied draft is wrong',
    );
    button('Next cues', dialog()!).click();
    await until(
      () =>
        dialog()!.querySelector('tbody') !== null &&
        dialog()!.textContent!.includes('101–200 of 205'),
    );
    button('Close', dialog()!).click();
    await until(() => !dialog() && !menu!.busy);
    assert(
      doc.cues[0]?.startMs === 1000 && !occluded,
      'Review close changed timings or left video occluded',
    );
    passed(
      'App shows unresolved and paged cue review without changing accepted timings',
    );
    incomplete = false;
    await open();
    await analyze();
    assert(
      dialog()!.textContent?.includes('/source.mkv') &&
        dialog()!.textContent?.includes('/target.mkv'),
      'Video roles are missing',
    );
    button('Accept timings', dialog()!).click();
    await until(() => !dialog() && !menu!.busy && doc.requiresSaveAs);
    assert(
      doc.cues.every((cue, i) => cue.startMs === 1500 + i * 2000) &&
        files.get('/original.srt') === original,
      'Acceptance changed source or missed a cue',
    );
    assert(
      host.textContent?.includes('Save As required'),
      'Save protection is not visible',
    );
    passed(
      'App accepts all timings through one command and displays Save As protection',
    );
    (document.activeElement as HTMLElement | null)?.blur();
    dispatch('undo');
    await until(() => !menu!.busy && doc.canRedo);
    assert(
      doc.cues.every((cue, i) => cue.startMs === 1000 + i * 2000) &&
        doc.requiresSaveAs,
      'Undo did not restore all cues or dropped protection',
    );
    dispatch('redo');
    await until(() => !menu!.busy && !doc.canRedo);
    passed(
      'one App Undo/Redo restores every cue while retaining source protection',
    );
    savePath = null;
    const saves = calls.filter((call) => call === 'document_action').length;
    dispatch('save');
    await until(() => !menu!.busy);
    assert(
      calls.filter((call) => call === 'document_action').length === saves &&
        doc.requiresSaveAs,
      'Cancelled Save As wrote a file',
    );
    savePath = '/original.srt';
    dispatch('save');
    await until(
      () =>
        !menu!.busy && host.textContent!.includes('Choose a different file'),
    );
    savePath = '/target.srt';
    failSave = true;
    dispatch('saveProject');
    await until(
      () => !menu!.busy && host.textContent!.includes('Injected save failure'),
    );
    assert(
      projectSubtitle === undefined &&
        doc.requiresSaveAs &&
        files.get('/original.srt') === original,
      'Failed Save As published a project or modified the original',
    );
    failSave = false;
    dispatch('saveProject');
    await until(() => !menu!.busy && projectSubtitle === '/target.srt');
    assert(
      !doc.requiresSaveAs &&
        files.has('/target.srt') &&
        files.get('/original.srt') === original,
      'Project did not continue with the new subtitle path',
    );
    passed(
      'Save As cancel/failure preserves state; project save continues only after a distinct subtitle save',
    );
    await open();
    phase = 'analyzing';
    button('Choose source video…', dialog()!).click();
    await until(() => !button('Analyze videos', dialog()!).disabled);
    button('Analyze videos', dialog()!).click();
    await until(() => !!dialog()!.textContent?.includes('Matching content'));
    button('Cancel operation', dialog()!).click();
    await until(() => !!dialog()!.textContent?.includes('Analysis cancelled'));
    assert(
      button('Accept timings', dialog()!).disabled,
      'Cancelled analysis can apply',
    );
    button('Close', dialog()!).click();
    await until(() => !dialog() && !menu!.busy);
    passed(
      'App cancellation retains accepted subtitles and restores native visibility on close',
    );
  } finally {
    await unmount(app);
  }
}

async function run() {
  await ownerCases();
  await appCases();
  heading.textContent = `PASS: all ${output.children.length} checks`;
}
void run().catch((error) => {
  const row = document.createElement('li');
  row.textContent = `FAIL: ${String(error)}`;
  output.append(row);
  heading.textContent = `FAIL: ${String(error)}`;
  console.error(error);
});
