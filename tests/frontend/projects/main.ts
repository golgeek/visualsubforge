import type { updateMenu } from '../../../src/lib/client';
import { defaultPreferences } from '../../../src/lib/client';
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
let projectSaveError: string | null = null;
let chosenPath: string | null = '/work.vsfproject';
let occluded = false;
let loadingPolls = 0;
const calls: string[] = [];
const saves: Record<string, unknown>[] = [];
const callbacks = new Map<number, (value: unknown) => void>();
let eventHandler = 0;
const project: ProjectData = {
  version: 1,
  mediaPath: '/clip.mkv',
  subtitlePath: '/captions.srt',
  mediaIdentity: { size: 10, modified: '1', sampleHash: 'abc' },
  positionSeconds: 12.5,
  options: {
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
        action?: MediaAction | DocumentAction;
        handler?: number;
        state?: Parameters<typeof updateMenu>[0];
        request?: Record<string, unknown>;
      },
    ) {
      calls.push(command);
      if (command === 'preferences_get')
        return structuredClone(defaultPreferences);
      switch (command) {
        case 'plugin:event|listen':
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
          if (failOpen) throw new Error('Project media has changed');
          return { ...structuredClone(project), token: 41 };
        case 'project_attach':
          return { path: '/work.vsfproject', token: 41 };
        case 'project_detach':
          return;
        case 'project_save':
          if (projectSaveError) throw new Error(projectSaveError);
          saves.push(args.request!);
          return { path: args.request!.path, token: 42 };
        case 'scenes_restore':
          if (!media.loaded)
            throw new Error('Scene restore before video loaded');
          return 4;
        case 'scenes_view':
          return {
            generation: 4,
            mode: 'visual',
            scanning: null,
            running: false,
            error: null,
            total: 2,
            truncated: false,
            cuts: [1, 20],
            previous: 1,
            next: 20,
          };
        case 'scenes_start':
          return 99;
        case 'media_action': {
          const action = args.action as MediaAction;
          if (action.kind === 'occlude') occluded = action.occluded;
          if (action.kind === 'open') {
            loadingPolls = 0;
            media = {
              ...media,
              session: media.session + 1,
              loaded: false,
              paused: true,
              durationSeconds: 100,
              positionSeconds: 0,
            };
          }
          if (
            action.kind === 'status' &&
            media.session > 0 &&
            ++loadingPolls >= 2
          )
            media.loaded = true;
          if (action.kind === 'seek') {
            if (!media.loaded) throw new Error('Seek before video loaded');
            media.positionSeconds = action.seconds;
          }
          return structuredClone(media);
        }
        case 'document_action': {
          const action = args.action as DocumentAction;
          if (action.kind === 'open')
            doc = {
              ...doc,
              loaded: true,
              path: '/captions.srt',
              generation: 1,
              cues: [
                { id: 1, startMs: 1000, endMs: 2000, text: 'Project cue' },
              ],
            };
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
  async function freshProject() {
    await unmount(app);
    menuState = undefined;
    media = { ...emptyStatus, available: true, surfaceAboveWebview: true };
    doc = structuredClone(emptyDocument);
    chosenPath = '/work.vsfproject';
    projectSaveError = null;
    calls.length = 0;
    saves.length = 0;
    app = mount(App, { target: host });
    await until(() => !!menuState && !menuState.busy && menuState.available);
    menuAction('openProject');
    await until(
      () =>
        !!host.textContent?.includes('Project restored') ||
        !!host.textContent?.includes('keyframe markers skipped'),
    );
    await until(() => !menuState?.busy);
  }
  try {
    await until(() => !(!menuState || menuState.busy || !menuState.available));
    menuAction('openProject');
    await until(() => !!host.textContent?.includes('Project restored'));
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
      JSON.stringify(saved.options) === JSON.stringify(project.options),
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
    const restores = calls.filter((c) => c === 'scenes_restore').length;
    menuAction('openProject');
    await until(() => !!host.textContent?.includes('keyframe markers skipped'));
    assert(
      calls.filter((c) => c === 'scenes_restore').length === restores &&
        calls.includes('scenes_start') &&
        !host.querySelector('[aria-label="Next scene change"]') &&
        media.positionSeconds === 12.5 &&
        doc.loaded,
      'Legacy project ignores cached keyframes and starts fresh visual analysis',
    );
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
    document.querySelector('#result')!.textContent =
      'PASS: all 18 project checks';
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
