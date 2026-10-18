import type { updateMenu } from '../../../src/lib/client';
import { defaultPreferences } from '../../../src/lib/client';
import type {
  LegacyImportReview,
  ProjectReferenceIssue,
  ProjectReferenceOverride,
  TranslationAction,
  TranslationReply,
} from '../../../src/lib/client';
import { legacyImportReview } from '../ipc-contract-samples';
import { mount, unmount, tick } from 'svelte';
import { isTauri } from '@tauri-apps/api/core';
import App from '../../../src/App.svelte';
import { defaultTiming } from '../../../src/lib/magnetic-timing';
import {
  emptyDocument,
  emptyStatus,
  type MediaAction,
  type DocumentAction,
  type ProjectData,
} from '../../../src/lib/client';
if (isTauri()) throw new Error('Browser fixture only');
const host = document.querySelector('#test')!;
let media = { ...emptyStatus, available: true };
let doc = structuredClone(emptyDocument);
let failOpen = false;
let restoreFailure: 'readiness' | 'subtitles' | 'scenes' | 'seek' | null = null;
let projectSaveError: string | null = null;
let chosenPath: string | null = '/work.vsfproject';
let occluded = false;
let importView: LegacyImportReview = structuredClone(legacyImportReview);
let openedPath: string | undefined;
let loadingPolls = 0;
let holdSceneCompletion = false;
let mediaOpening = false;
let referenceIssue: ProjectReferenceIssue | null = null;
const referenceRequests: (ProjectReferenceOverride | null)[] = [];
const referenceSummary = {
  epoch: 1,
  dirty: false,
  reference: {
    path: '/reference.srt',
    format: 'srt',
    sha256: 'a'.repeat(64),
    cues: 1,
    offsetMs: -1500,
  },
};
let attachReference = false;
let translationJob: TranslationReply['job'] = null;
let translationId = 0;
let holdReferenceLoad = false;
const referenceRows = Array.from({ length: 201 }, (_, index) => ({
  index,
  startMs: index * 1000 - 500,
  endMs: index * 1000 + 500,
  overlapMs: 500,
  raw:
    index === 0
      ? '{\\an8}<i>Été 世界</i><img src=x onerror=alert(1)>'
      : `Reference ${index + 1}`,
  reading: index === 0 ? 'Été 世界' : `Reference ${index + 1}`,
  warning: index === 0 ? 'Layout is omitted from the reading view' : null,
  timed: false,
  layout: index === 0,
  truncated: false,
}));
const calls: string[] = [];
const saves: Record<string, unknown>[] = [];
const callbacks = new Map<number, (value: unknown) => void>();
let eventHandler = 0;
const observeAfterSceneCompletion = new URLSearchParams(location.search).has(
  'observeAfterSceneCompletion',
);
// Delay the actual IPC registration response, independent of module loading.
const registration = new URLSearchParams(location.search).has('delayListener')
  ? new Promise<void>((resolve) => {
      window.addEventListener('vsf-test-release-listener', () => resolve(), {
        once: true,
      });
    })
  : Promise.resolve();
const project: ProjectData = {
  version: 1,
  mediaPath: '/clip.mkv',
  subtitlePath: '/captions.srt',
  mediaIdentity: { size: 10, modified: '1', sampleHash: 'abc' },
  positionSeconds: 12.5,
  options: {
    quality: null,
    waveformStart: 10,
    waveformSpan: 20,
    followPlayback: false,
    followSubtitle: false,
    showScenes: true,
    timing: { ...defaultTiming },
  },
  scenes: {
    version: 1,
    mode: 'visual',
    cuts: [1, 20],
    identity: { size: 10, modified: '1', sampleHash: 'abc' },
  },
};
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
        action?: MediaAction | DocumentAction | TranslationAction;
        handler?: number;
        generation?: number;
        state?: Parameters<typeof updateMenu>[0];
        request?: Record<string, unknown>;
        path?: string;
        reference?: ProjectReferenceOverride | null;
      },
    ) {
      calls.push(command);
      if (command === 'preferences_get')
        return structuredClone(defaultPreferences);
      switch (command) {
        case 'plugin:event|listen':
          document.body.dataset.workspaceListener = 'waiting';
          await registration;
          document.body.dataset.workspaceListener = 'registered';
          eventHandler = args.handler!;
          return 1;
        case 'plugin:event|unlisten':
          return;
        case 'update_menu':
          menuState = args.state;
          return;
        case 'recovery_status':
          return { entries: [], error: null };
        case 'choose_file':
          return chosenPath;
        case 'project_open':
          openedPath = args.path;
          if (failOpen) throw new Error('Project media has changed');
          referenceRequests.push(args.reference ?? null);
          if (
            referenceIssue &&
            !args.reference?.reload &&
            !args.reference?.path
          )
            return { ...structuredClone(project), referenceIssue, token: 0 };
          return { ...structuredClone(project), token: 41 };
        case 'project_import_start':
          importView = structuredClone(legacyImportReview);
          return 1;
        case 'project_import_view':
          return structuredClone(importView);
        case 'project_import_publish':
          importView = {
            ...importView,
            phase: 'published',
            outputPath: args.path!,
          };
          return;
        case 'project_import_cancel':
          return;
        case 'project_attach':
          if (attachReference)
            doc.translation = structuredClone(referenceSummary);
          return { path: '/work.vsfproject', token: 41 };
        case 'project_detach':
          return;
        case 'project_save':
          if (projectSaveError) throw new Error(projectSaveError);
          saves.push(args.request!);
          if (doc.translation) doc.translation.dirty = false;
          return { path: args.request!.path, token: 42 };
        case 'translation_action': {
          const action = args.action as TranslationAction;
          const key = () => ({
            generation: doc.generation,
            revision: doc.revision,
            epoch: doc.translation?.epoch ?? 0,
          });
          let replacement = false;
          if (action.kind === 'load') {
            if (action.path === '/invalid.srt')
              throw new Error('Invalid reference file');
            translationJob = {
              id: ++translationId,
              key: key(),
              copy: action.copy,
              phase: 'working',
              reference: { ...referenceSummary.reference, path: action.path },
              page: null,
              error: null,
            };
          } else if (action.kind === 'query') {
            const rows =
              action.targetCueId === null
                ? referenceRows
                : action.targetCueId === 1
                  ? referenceRows.slice(0, 2)
                  : [];
            translationJob = {
              id: ++translationId,
              key: key(),
              copy: false,
              phase: 'ready',
              reference: null,
              page: {
                total: rows.length,
                offset: action.offset,
                rows: rows.slice(action.offset, action.offset + action.limit),
                selected:
                  action.selected === null
                    ? null
                    : (referenceRows[action.selected] ?? null),
              },
              error: null,
            };
          } else if (
            action.kind === 'cancel' &&
            translationJob?.id === action.id
          )
            translationJob.phase = 'cancelled';
          else if (
            action.kind === 'view' &&
            translationJob?.id === action.id &&
            translationJob.phase === 'working' &&
            !holdReferenceLoad
          )
            translationJob.phase = 'ready';
          else if (
            action.kind === 'accept' &&
            translationJob?.id === action.id
          ) {
            if (translationJob.phase !== 'ready' || !translationJob.reference)
              throw new Error('Reference is not ready');
            doc.translation = {
              epoch: key().epoch + 1,
              dirty: true,
              reference: translationJob.reference,
            };
            if (translationJob.copy) {
              replacement = true;
              doc = {
                ...doc,
                generation: doc.generation + 1,
                revision: 0,
                path: null,
                loaded: true,
                requiresSaveAs: true,
                dirty: false,
                canUndo: false,
                canRedo: false,
                cues: [
                  {
                    id: 1,
                    startMs: 1000,
                    endMs: 2000,
                    text: 'Copied original text',
                  },
                ],
              };
            }
            translationJob.phase = 'accepted';
          } else if (action.kind === 'setOffset' && doc.translation?.reference)
            doc.translation = {
              ...doc.translation,
              epoch: key().epoch + 1,
              dirty: true,
              reference: {
                ...doc.translation.reference,
                offsetMs: action.offsetMs,
              },
            };
          else if (action.kind === 'detach')
            doc.translation = {
              epoch: key().epoch + 1,
              dirty: true,
              reference: null,
            };
          return structuredClone({
            key: key(),
            context: doc.translation ?? null,
            job: translationJob,
            document: replacement ? doc : null,
          } satisfies TranslationReply);
        }
        case 'scenes_restore':
          if (restoreFailure === 'scenes')
            throw new Error('Injected scenes failure');
          if (!media.loaded)
            throw new Error('Scene restore before video loaded');
          return 4;
        case 'scenes_view':
          return {
            generation: args.generation,
            mode: holdSceneCompletion ? null : 'visual',
            scanning: holdSceneCompletion ? 'visual' : null,
            running: holdSceneCompletion,
            error: null,
            total: holdSceneCompletion ? 0 : 2,
            truncated: false,
            cuts: holdSceneCompletion ? [] : [1, 20],
            previous: holdSceneCompletion ? null : 1,
            next: holdSceneCompletion ? null : 20,
          };
        case 'scenes_start':
          return 99;
        case 'media_action': {
          const action = args.action as MediaAction;
          if (action.kind === 'occlude') occluded = action.occluded;
          if (action.kind === 'open') {
            loadingPolls = 0;
            mediaOpening = true;
            media = {
              ...media,
              session: media.session + 1,
              loaded: false,
              paused: true,
              error: null,
              durationSeconds: 100,
              positionSeconds: 0,
            };
          }
          if (action.kind === 'close') {
            mediaOpening = false;
            media = {
              ...emptyStatus,
              available: media.available,
              session: media.session + 1,
            };
          }
          if (action.kind === 'status' && mediaOpening && ++loadingPolls >= 2) {
            if (restoreFailure === 'readiness')
              media.error = 'Injected readiness failure';
            else {
              media.loaded = true;
              mediaOpening = false;
            }
          }
          if (action.kind === 'seek') {
            if (restoreFailure === 'seek')
              throw new Error('Injected seek failure');
            if (!media.loaded) throw new Error('Seek before video loaded');
            media.positionSeconds = action.seconds;
          }
          return structuredClone(media);
        }
        case 'document_action': {
          const action = args.action as DocumentAction;
          if (action.kind === 'open') {
            if (restoreFailure === 'subtitles')
              throw new Error('Injected subtitles failure');
            doc = {
              ...doc,
              translation: null,
              loaded: true,
              path: '/captions.srt',
              generation: 1,
              cues: [
                { id: 1, startMs: 1000, endMs: 2000, text: 'Project cue' },
              ],
            };
          }
          return structuredClone(doc);
        }
        case 'finish_exit':
          return;
        default:
          throw new Error(command);
      }
    },
  },
});
let menuState: Parameters<typeof updateMenu>[0] | undefined;
function workspaceReady() {
  return callbacks.has(eventHandler) && !!menuState && !menuState.busy;
}
function menuAction(action: string) {
  callbacks.get(eventHandler)!({ payload: action });
}
async function until(check: () => boolean) {
  const deadline = performance.now() + 5000;
  await tick();
  while (!check()) {
    if (performance.now() > deadline)
      throw new Error('UI timeout: ' + host.textContent);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
function assert(ok: boolean, text: string) {
  if (!ok) throw new Error(text);
  const row = document.createElement('li');
  row.textContent = 'PASS: ' + text;
  document.querySelector('#checks')!.append(row);
}
async function run() {
  let app = mount(App, { target: host });
  function button(text: string) {
    const button = [...host.querySelectorAll('button')].find(
      (button) => button.textContent?.trim() === text,
    );
    if (!button) throw new Error(`Missing button: ${text}`);
    button.click();
  }
  function exitDialog() {
    return host.querySelector<HTMLDialogElement>(
      'dialog[aria-labelledby="project-save-failure-title"][open]',
    );
  }
  async function restoreProject() {
    const attachments = calls.filter(
      (call) => call === 'project_attach',
    ).length;
    menuAction('openProject');
    if (
      observeAfterSceneCompletion &&
      project.scenes?.mode === 'keyframes' &&
      !holdSceneCompletion
    ) {
      // Model a busy event loop: the new scan can replace the restoration
      // notice before the harness gets its next turn to observe the UI.
      await until(
        () =>
          host.querySelector('footer [role="status"]')?.textContent ===
          '2 visual cuts',
      );
    }
    // Status notices can be replaced by background analysis. Attachment plus
    // an idle workspace proves this open completed, even after that message.
    await until(
      () =>
        calls.filter((call) => call === 'project_attach').length >
          attachments && workspaceReady(),
    );
  }
  async function freshProject(available = true) {
    await unmount(app);
    menuState = undefined;
    eventHandler = 0;
    media = { ...emptyStatus, available, surfaceAboveWebview: true };
    mediaOpening = false;
    doc = structuredClone(emptyDocument);
    chosenPath = '/work.vsfproject';
    projectSaveError = null;
    restoreFailure = null;
    calls.length = 0;
    saves.length = 0;
    app = mount(App, { target: host });
    await until(workspaceReady);
    await restoreProject();
  }
  try {
    await until(workspaceReady);
    await restoreProject();
    await until(() => calls.includes('scenes_view'));
    assert(
      media.positionSeconds === 12.5 &&
        media.paused &&
        doc.cues[0]?.text === 'Project cue',
      'Open waits for video readiness before restoring position and subtitles',
    );
    assert(
      !calls.includes('scenes_start'),
      'Saved visual results restore without rescanning',
    );
    menuAction('saveProject');
    await until(
      () =>
        saves.length === 1 &&
        !(!menuState || menuState.busy || !menuState.media),
    );
    const saved = saves[0]!;
    assert(
      saved.path === '/work.vsfproject' &&
        saved.token === 41 &&
        saved.positionSeconds === 12.5 &&
        saved.subtitlePath === '/captions.srt',
      'Save uses attached path, Rust token and current workspace',
    );
    assert(
      Object.entries(project.options)
        .filter(([key]) => key !== 'quality')
        .every(
          ([key, value]) =>
            JSON.stringify((saved.options as Record<string, unknown>)[key]) ===
            JSON.stringify(value),
        ) && !('quality' in (saved.options as Record<string, unknown>)),
      'Waveform and follow options roundtrip',
    );
    projectSaveError =
      'Project changed on disk. Reopen it or use Save Project As to preserve both versions';
    menuAction('saveProject');
    await until(() => !!host.textContent?.includes(projectSaveError!));
    assert(
      saves.length === 1 && doc.loaded,
      'Conflicting project save reports failure without replacing the workspace',
    );
    await until(() => !menuState?.busy);
    projectSaveError = null;
    chosenPath = '/copy.vsfproject';
    menuAction('saveProjectAs');
    await until(() => saves.length === 2 && !menuState?.busy);
    assert(
      saves[1]?.path === chosenPath && saves[1]?.token === 42,
      'Save As retains the current Rust save token after a conflict',
    );
    chosenPath = '/work.vsfproject';
    failOpen = true;
    const session = media.session;
    menuAction('openProject');
    await until(
      () => !!host.textContent?.includes('Project media has changed'),
    );
    assert(
      media.session === session && media.positionSeconds === 12.5 && doc.loaded,
      'Invalid project leaves current workspace intact',
    );
    failOpen = false;
    project.scenes!.mode = 'keyframes';
    // Keep the new visual scan incomplete while asserting that old keyframes
    // are hidden. Fast polling must not turn this into a check of fresh cuts.
    holdSceneCompletion = true;
    const restores = calls.filter((c) => c === 'scenes_restore').length;
    await restoreProject();
    await until(() => !!menuState?.scenesRunning);
    assert(
      calls.filter((c) => c === 'scenes_restore').length === restores &&
        calls.includes('scenes_start') &&
        !host.querySelector('[aria-label="Next scene change"]') &&
        media.positionSeconds === 12.5 &&
        doc.loaded,
      'Legacy project ignores cached keyframes and starts fresh visual analysis',
    );
    holdSceneCompletion = false;
    callbacks.get(eventHandler)!({ payload: 'quit' });
    await until(() => calls.includes('finish_exit'));
    assert(
      saves.length === 3 &&
        calls.lastIndexOf('project_save') < calls.lastIndexOf('finish_exit'),
      'Normal exit saves project before shutdown',
    );

    await freshProject();
    projectSaveError = 'Cannot save project: permission denied';
    menuAction('quit');
    await until(() => !!exitDialog());
    assert(
      occluded && !calls.includes('finish_exit'),
      'Failed quit save opens choices above occluded native video',
    );
    assert(
      document.activeElement?.textContent === 'Cancel Quit',
      'Quit failure defaults keyboard focus to Cancel Quit',
    );
    exitDialog()!.dispatchEvent(new Event('cancel', { cancelable: true }));
    await until(() => !menuState?.busy);
    assert(
      !exitDialog() && !occluded && !calls.includes('finish_exit'),
      'Escape cancels quitting and restores native video',
    );

    menuAction('quit');
    await until(() => !!exitDialog());
    chosenPath = null;
    button('Save Project As…');
    await until(() => !menuState?.busy);
    assert(
      !calls.includes('finish_exit') && !exitDialog(),
      'Cancelling Save As cancels quit without discarding the workspace',
    );

    menuAction('quit');
    await until(() => !!exitDialog());
    const failedAttempts = calls.filter(
      (call) => call === 'project_save',
    ).length;
    button('Retry');
    await until(
      () =>
        calls.filter((call) => call === 'project_save').length >
          failedAttempts && !!exitDialog(),
    );
    assert(
      !calls.includes('finish_exit'),
      'A failed retry returns to the quit choices',
    );
    projectSaveError = null;
    button('Retry');
    await until(() => calls.includes('finish_exit'));
    assert(
      saves.length === 1 && !exitDialog() && !occluded,
      'Successful retry saves the project before normal shutdown',
    );

    await freshProject();
    projectSaveError = 'Project changed on disk';
    menuAction('quit');
    await until(() => !!exitDialog());
    projectSaveError = null;
    chosenPath = '/quit-copy.vsfproject';
    button('Save Project As…');
    await until(() => calls.includes('finish_exit'));
    assert(
      saves[0]?.path === chosenPath,
      'Quit can save a separate project copy after a conflict',
    );

    await freshProject();
    const draft = host.querySelector<HTMLTextAreaElement>(
      'textarea[aria-label="Subtitle text"]',
    )!;
    draft.value = 'Unapplied draft';
    draft.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    projectSaveError = 'Cannot save project: permission denied';
    menuAction('quit');
    await until(
      () =>
        !!host.querySelector(
          'dialog[aria-label="Unsaved subtitle changes"][open]',
        ),
    );
    button('Discard changes');
    await until(() => !!exitDialog());
    button('Quit Without Saving Project');
    await until(() => calls.includes('finish_exit'));
    assert(
      saves.length === 0 && draft.value === 'Unapplied draft',
      'Explicit quit without project save preserves the earlier subtitle discard decision',
    );

    await freshProject();
    projectSaveError = 'Cannot save project';
    menuAction('quit');
    await until(() => !!exitDialog());
    // Teardown resolves the pending decision as cancellation, never as permission to exit.
    await unmount(app);
    await tick();
    assert(
      !calls.includes('finish_exit'),
      'Unmount cancels the pending quit decision',
    );
    app = mount(App, { target: host });
    project.scenes!.mode = 'visual';
    for (const stage of ['readiness', 'subtitles', 'scenes', 'seek'] as const) {
      await freshProject();
      const previousSession = media.session;
      calls.length = 0;
      restoreFailure = stage;
      menuAction('openProject');
      await until(
        () => !!host.textContent?.includes(`Injected ${stage} failure`),
      );
      await until(() => !menuState?.busy);
      menuAction('quit');
      await until(() => calls.includes('finish_exit'));
      assert(
        media.session > previousSession &&
          !calls.includes('project_attach') &&
          !calls.includes('project_save'),
        `Failed ${stage} restore leaves no attachment to autosave on quit`,
      );
      restoreFailure = null;
      await until(() => !menuState?.busy);
      await restoreProject();
      assert(
        calls.includes('project_attach') &&
          media.positionSeconds === 12.5 &&
          doc.loaded,
        `Project restoration can retry after ${stage} failure`,
      );
    }
    await freshProject();
    const importDraft = host.querySelector<HTMLTextAreaElement>(
      'textarea[aria-label="Subtitle text"]',
    )!;
    importDraft.focus();
    importDraft.value = 'Keep this translation draft';
    importDraft.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    for (const decision of ['Cancel', 'Discard changes']) {
      const before = JSON.stringify(doc);
      const session = media.session;
      menuAction('importLegacyProject');
      await until(
        () =>
          !!host.querySelector(
            'dialog[aria-labelledby="legacy-import-title"][open]',
          ),
      );
      assert(
        occluded && !!menuState?.busy,
        'Legacy import occludes native video and gates workspace commands',
      );
      chosenPath = '/old.vssprj';
      button('Choose legacy project…');
      await until(() => host.textContent!.includes('/old.vssprj'));
      button('Review import');
      await until(
        () => !!host.querySelector('[aria-label="Legacy import review"]'),
      );
      chosenPath = '/imported.vsfproject';
      button('Create new VSF project…');
      await until(() => !!host.querySelector('[aria-label="Project created"]'));
      assert(
        JSON.stringify(doc) === before &&
          media.session === session &&
          importDraft.value === 'Keep this translation draft',
        'Creating an import preserves the active document, media and unapplied draft',
      );
      const picks = calls.filter((call) => call === 'choose_file').length;
      button('Open created project');
      await until(
        () =>
          !!host.querySelector(
            'dialog[aria-label="Unsaved subtitle changes"][open]',
          ),
      );
      assert(
        openedPath === '/imported.vsfproject' &&
          calls.filter((call) => call === 'choose_file').length === picks,
        'Open created project preflights the exact output path without a second picker',
      );
      button(decision);
      await until(workspaceReady);
      assert(
        !occluded &&
          !host.querySelector('dialog[aria-labelledby="legacy-import-title"]'),
        'Closing import restores video and releases its workspace gate',
      );
      assert(
        decision === 'Cancel'
          ? media.session === session &&
              importDraft.value === 'Keep this translation draft'
          : media.session > session && doc.cues[0]?.text === 'Project cue',
        `Explicit ${decision} protects or restores the imported project through the existing workflow`,
      );
    }
    await freshProject();
    const referenceDraft = host.querySelector<HTMLTextAreaElement>(
      'textarea[aria-label="Subtitle text"]',
    )!;
    referenceDraft.value = 'Protect this draft while choosing a reference';
    referenceDraft.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    referenceIssue = {
      path: '/reference.srt',
      message: 'Reference content changed',
      canReload: true,
    };
    const beforeReferenceSession = media.session;
    const referenceDialog = () =>
      host.querySelector<HTMLDialogElement>(
        'dialog[aria-labelledby="project-reference-title"][open]',
      );
    menuAction('openProject');
    await until(() => !!referenceDialog());
    assert(
      occluded &&
        media.session === beforeReferenceSession &&
        referenceDraft.value.startsWith('Protect this draft'),
      'Reference preflight asks before replacing media or the unapplied draft',
    );
    assert(
      document.activeElement?.textContent?.trim() === 'Cancel Open',
      'Reference resolution defaults focus to Cancel Open',
    );
    referenceDialog()!.dispatchEvent(new Event('cancel', { cancelable: true }));
    await until(workspaceReady);
    assert(
      !occluded &&
        media.session === beforeReferenceSession &&
        referenceDraft.value.startsWith('Protect this draft'),
      'Cancelling reference resolution leaves the active workspace intact',
    );

    menuAction('openProject');
    await until(() => !!referenceDialog());
    button('Reload Changed Reference');
    await until(
      () =>
        !!host.querySelector(
          'dialog[aria-label="Unsaved subtitle changes"][open]',
        ),
    );
    assert(
      referenceRequests.at(-1)?.reload === true &&
        media.session === beforeReferenceSession,
      'Explicit reload revalidates the reference before asking to replace the draft',
    );
    button('Cancel');
    await until(workspaceReady);
    referenceIssue = {
      ...referenceIssue,
      canReload: false,
      message: 'Reference unavailable',
    };
    menuAction('openProject');
    await until(() => !!referenceDialog());
    assert(
      !referenceDialog()!.textContent?.includes('Reload Changed Reference'),
      'An unavailable reference offers relocation without an invalid reload choice',
    );
    chosenPath = '/relocated.srt';
    button('Choose Reference…');
    await until(
      () =>
        !!host.querySelector(
          'dialog[aria-label="Unsaved subtitle changes"][open]',
        ),
    );
    assert(
      referenceRequests.at(-1)?.path === chosenPath &&
        referenceRequests.at(-1)?.reload === false,
      'Choosing a reference preserves identity checking instead of implicitly accepting changed contents',
    );
    button('Cancel');
    await until(workspaceReady);
    referenceIssue = null;

    attachReference = true;
    referenceSummary.dirty = true;
    await freshProject();
    const unsavedProject = () =>
      host.querySelector('dialog[aria-label="Unsaved project changes"][open]');
    menuAction('quit');
    await until(() => !!unsavedProject());
    button('Cancel');
    await until(workspaceReady);
    assert(
      !calls.includes('finish_exit') && doc.translation?.dirty === true,
      'Reference-only changes require an explicit save or discard before quitting',
    );
    menuAction('quit');
    await until(() => !!unsavedProject());
    button('Discard changes');
    await until(() => calls.includes('finish_exit'));
    assert(
      saves.length === 0,
      'Discarding reference changes on quit does not autosave those changes',
    );

    await freshProject();
    menuAction('quit');
    await until(() => !!unsavedProject());
    button('Save project');
    await until(() => calls.includes('finish_exit'));
    assert(
      saves.length === 1 && doc.translation?.dirty === false,
      'Saving reference changes refreshes accepted state and saves once before quitting',
    );

    await freshProject();
    const beforeOpens = calls.filter((call) => call === 'project_open').length;
    const beforeAttachments = calls.filter(
      (call) => call === 'project_attach',
    ).length;
    menuAction('openProject');
    await until(() => !!unsavedProject());
    button('Save project');
    await until(
      () =>
        calls.filter((call) => call === 'project_attach').length >
          beforeAttachments && workspaceReady(),
    );
    assert(
      saves.length === 1 &&
        calls.filter((call) => call === 'project_open').length ===
          beforeOpens + 2,
      'Saving the current context restages the next project before replacing the workspace',
    );

    referenceSummary.dirty = false;
    project.version = 2;
    project.mediaPath = null;
    project.mediaIdentity = null;
    project.positionSeconds = 0;
    project.scenes = null;
    for (const available of [true, false]) {
      await freshProject(available);
      assert(
        doc.loaded &&
          !media.loaded &&
          !calls.includes('scenes_restore') &&
          doc.translation?.reference?.offsetMs === -1500,
        `Subtitle-only project restores its target and reference with playback available=${available}`,
      );
      menuAction('saveProject');
      await until(() => saves.length === 1 && workspaceReady());
      assert(
        saves[0]?.mediaPath === null && saves[0]?.positionSeconds === 0,
        `Subtitle-only save omits media and position with playback available=${available}`,
      );
    }
    attachReference = false;
    await freshProject();
    menuAction('translationReference');
    await until(
      () => !!host.querySelector('[aria-label="Translation reference"]'),
    );
    const targetDraft = host.querySelector<HTMLTextAreaElement>(
      'textarea[aria-label="Subtitle text"]',
    )!;
    targetDraft.value = 'Keep my translated draft';
    targetDraft.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    chosenPath = '/reference.ass';
    button('Attach reference…');
    await until(() => !!doc.translation?.reference && workspaceReady());
    await until(
      () => !!host.querySelector('[aria-label="Reference cues"] button'),
    );
    assert(
      targetDraft.value === 'Keep my translated draft' &&
        host.textContent!.includes('unapplied draft'),
      'Attaching a reference preserves the target draft and labels accepted-timing matches',
    );
    host
      .querySelector<HTMLButtonElement>('[aria-label="Reference cues"] button')!
      .click();
    await until(
      () => !!host.querySelector('[aria-label="Selected reference cue"]'),
    );
    const selectedReference = host.querySelector(
      '[aria-label="Selected reference cue"]',
    )!;
    assert(
      selectedReference.textContent!.includes('{\\an8}<i>Été 世界</i><img') &&
        !selectedReference.querySelector('img'),
      'Reference raw markup is literal text alongside its reading projection',
    );
    const offsetInput = host.querySelector<HTMLInputElement>(
      '[aria-label="Reference offset in milliseconds"]',
    )!;
    offsetInput.value = '-2500';
    offsetInput.dispatchEvent(new Event('input', { bubbles: true }));
    button('Apply offset');
    await until(
      () => doc.translation?.reference?.offsetMs === -2500 && workspaceReady(),
    );
    assert(
      targetDraft.value === 'Keep my translated draft' &&
        doc.cues[0]?.startMs === 1000,
      'Signed reference offsets preserve target timing and its unapplied draft',
    );
    const referenceView = host.querySelector<HTMLSelectElement>(
      '[aria-label="Reference view"]',
    )!;
    referenceView.selectedIndex = 1;
    referenceView.dispatchEvent(new Event('change', { bubbles: true }));
    await until(() => host.textContent!.includes('201 reference cues'));
    button('Next reference page');
    await until(
      () =>
        host
          .querySelector('[aria-label="Reference cues"] button')
          ?.textContent?.includes('#101') === true,
    );
    assert(
      host.querySelectorAll('[aria-label="Reference cues"] button').length ===
        100,
      'Reference browsing pages bounded rows independently of the selected target',
    );
    button('Close reference panel');
    await until(
      () => !host.querySelector('[aria-label="Translation reference"]'),
    );
    assert(
      !!doc.translation?.reference &&
        targetDraft.value === 'Keep my translated draft',
      'Closing the reference panel preserves accepted context and the target draft',
    );
    menuAction('translationReference');
    await until(
      () => !!host.querySelector('[aria-label="Translation reference"]'),
    );
    chosenPath = '/invalid.srt';
    button('Replace reference…');
    await until(
      () =>
        host.textContent!.includes('Invalid reference file') &&
        workspaceReady(),
    );
    assert(
      doc.translation?.reference?.path === '/reference.ass' &&
        targetDraft.value === 'Keep my translated draft',
      'Failed reference replacement preserves the original context and draft',
    );
    holdReferenceLoad = true;
    chosenPath = '/cancelled.srt';
    button('Replace reference…');
    await until(() => translationJob?.phase === 'working');
    button('Cancel reference operation');
    await until(workspaceReady);
    holdReferenceLoad = false;
    assert(
      doc.translation?.reference?.path === '/reference.ass' &&
        targetDraft.value === 'Keep my translated draft',
      'Cancelling reference preparation does not attach it or replace the draft',
    );
    button('Detach reference');
    await until(() => !doc.translation?.reference && workspaceReady());
    assert(
      targetDraft.value === 'Keep my translated draft',
      'Reference detachment preserves the target draft',
    );
    chosenPath = '/original.srt';
    button('Start target copy…');
    await until(() => !!unsavedProject());
    button('Cancel');
    await until(workspaceReady);
    assert(
      targetDraft.value === 'Keep my translated draft',
      'Cancelling target replacement retains the current translation',
    );
    button('Start target copy…');
    await until(() => !!unsavedProject());
    button('Discard changes');
    await until(() => doc.requiresSaveAs && workspaceReady());
    assert(
      targetDraft.value === 'Copied original text' &&
        doc.path === null &&
        doc.translation?.reference?.path === '/original.srt',
      'An authorized independent target copy requires Save As and retains its original as reference',
    );
    document.querySelector('#result')!.textContent =
      `PASS: all ${document.querySelectorAll('#checks li').length} project checks`;
  } catch (e) {
    document.querySelector('#result')!.textContent = 'FAIL: ' + String(e);
  } finally {
    await unmount(app);
    for (const key of [
      'isTauri',
      '__TAURI_INTERNALS__',
      '__TAURI_EVENT_PLUGIN_INTERNALS__',
    ])
      Reflect.deleteProperty(window, key);
  }
}
void run();
