import { mount, tick, unmount } from 'svelte';
import { WindowChrome } from '../../../src/lib/workspace/window-chrome.svelte';
import WorkspaceToolbar from '../../../src/lib/workspace/WorkspaceToolbar.svelte';
import { Workspace } from '../../../src/lib/workspace/workspace.svelte';
import { DocumentSession } from '../../../src/lib/document/document-session.svelte';
import { MediaSession } from '../../../src/lib/media/media-session.svelte';
import { WorkspaceModal } from '../../../src/lib/dialogs/workspace-modal.svelte';
import ActionBrowser from '../../../src/lib/workspace/ActionBrowser.svelte';
import type { ActionContext } from '../../../src/lib/workspace/action-registry';
import type { WindowPlatform } from '../../../src/lib/client';
import '../../../src/lib/ui/tokens.css';
import '../../../src/lib/ui/base.css';
const host = document.querySelector<HTMLElement>('#test')!;
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const li = document.createElement('li');
  li.textContent = 'PASS: ' + message;
  document.querySelector('#checks')!.append(li);
}
function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((yes) => {
    resolve = yes;
  });
  return { resolve, promise };
}
async function until(condition: () => boolean) {
  const deadline = performance.now() + 5000;
  while (!condition()) {
    assert(performance.now() < deadline, 'Chrome did not settle');
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await tick();
}
function fixture(platform: WindowPlatform) {
  const order: string[] = [];
  let hidden = false;
  let remaining = false;
  let failure = false;
  let readFailure = false;
  let decorated = true;
  let hold: Promise<void> | null = null;
  const actions: string[] = [];
  const workspace = new Workspace({
    activity: () => false,
    confirming: () => false,
    run: async (action) => {
      actions.push(action);
    },
  });
  workspace.ready = true;
  const effects = {
    occlude: async (value: boolean) => {
      order.push('hide:' + value);
      hidden = value;
      if (hold) await hold;
    },
    remainingOcclusion: () => remaining,
    afterRender: async () => {
      order.push('bounds');
      await tick();
    },
  };
  const api = {
    setWindowDecorations: async (value: boolean) => {
      order.push('decorations:' + value);
      decorated = value;
      if (failure) throw new Error('Unknown native outcome');
    },
    windowIsDecorated: async () => {
      order.push('read');
      if (readFailure) throw new Error('Read unavailable');
      return decorated;
    },
    dragWindow: async () => {
      actions.push('drag');
    },
    minimizeWindow: async () => {
      actions.push('minimize');
    },
    toggleWindowMaximized: async () => {
      order.push('maximize');
    },
  };
  const chrome = new WindowChrome(workspace, effects, platform, true, api);
  return {
    chrome,
    workspace,
    effects,
    order,
    actions,
    get hidden() {
      return hidden;
    },
    set remaining(value: boolean) {
      remaining = value;
    },
    set failure(value: boolean) {
      failure = value;
    },
    set readFailure(value: boolean) {
      readFailure = value;
    },
    set hold(value: Promise<void> | null) {
      hold = value;
    },
  };
}
const context: ActionContext = {
  ready: true,
  busy: false,
  document: false,
  media: false,
  available: true,
  cue: false,
  cues: false,
  draft: false,
  styled: false,
  webvtt: false,
  undo: false,
  redo: false,
  range: false,
  scenesRunning: false,
  scenesPending: false,
  previousScene: false,
  nextScene: false,
  problems: false,
  previousProblem: false,
  nextProblem: false,
  operations: {},
};
async function run() {
  for (const platform of ['windows', 'macos', 'linux'] as const) {
    const f = fixture(platform);
    await f.chrome.start(platform !== 'linux');
    assert(
      f.order.length === 0 &&
        !f.chrome.custom &&
        f.chrome.overlay === (platform === 'macos'),
      'Defaults preserve native captions except mac overlay',
    );
    f.chrome.dispose();
  }
  passed(
    'Windows retains native captions, macOS reserves an overlay, and Linux defaults to native decorations',
  );
  const f = fixture('linux');
  await f.chrome.start(true);
  assert(
    f.order.join(',') === 'hide:true,decorations:false,bounds,hide:false' &&
      f.chrome.custom &&
      !f.workspace.busy,
    'Custom decoration transition ordered through native hide/render/restore',
  );
  await f.chrome.start(false);
  assert(
    f.chrome.custom && f.order.length === 4,
    'Preference changes require restart, not surprise resize',
  );
  passed(
    'Linux opt-in changes decorations once, with acknowledged bounds before restoring video',
  );
  const subtitles = new DocumentSession();
  const media = new MediaSession(() => {});
  let selected = '';
  const toolbar = mount(WorkspaceToolbar, {
    target: host,
    props: {
      chrome: f.chrome,
      workspace: f.workspace,
      subtitles,
      media,
      projects: { path: null },
      nativeMenus: true,
      diagnostics: false,
      showScenes: false,
      sceneState: { pending: false, running: false },
      oncommands: () => {},
      onshortcuts: () => {},
      onlayout: () => {},
      ongroup: (group) => {
        selected = group;
      },
    },
  });
  try {
    await tick();
    const menu = [...host.querySelectorAll('nav button')];
    assert(menu.length === 8, 'All application groups available');
    (menu[0] as HTMLButtonElement).click();
    assert(selected === 'File', 'Top bar routes group');
    const move = host.querySelector<HTMLButtonElement>('[data-window-drag]')!;
    move.dispatchEvent(
      new MouseEvent('mousedown', { bubbles: true, button: 0, detail: 1 }),
    );
    await until(() => f.actions.includes('drag'));
    host
      .querySelector<HTMLButtonElement>('[aria-label="Minimize window"]')!
      .click();
    await until(() => f.actions.includes('minimize'));
    host
      .querySelector<HTMLButtonElement>(
        '[aria-label="Maximize or restore window"]',
      )!
      .click();
    await until(() => !f.workspace.busy && f.order.includes('maximize'));
    host
      .querySelector<HTMLButtonElement>('[aria-label="Close window"]')!
      .click();
    await until(() => f.actions.includes('quit'));
    assert(
      !f.hidden &&
        f.order.slice(-4).join(',') === 'hide:true,maximize,bounds,hide:false',
      'Maximize has bounded native transition',
    );
    passed(
      'Custom captions route drag/minimize/maximize and protected Quit; top menus expose all groups',
    );
  } finally {
    await unmount(toolbar);
    subtitles.dispose();
    media.dispose();
    f.chrome.dispose();
  }
  const fail = fixture('linux');
  fail.failure = true;
  fail.readFailure = true;
  await fail.chrome.start(true);
  assert(
    fail.chrome.custom &&
      fail.workspace.error.includes('Unknown native outcome') &&
      !fail.hidden &&
      !fail.workspace.busy &&
      fail.order.filter((value) => value.startsWith('decorations')).length ===
        1,
    'Unknown decoration result retains controls without mutation replay',
  );
  fail.chrome.dispose();
  passed(
    'Unknown decoration outcomes keep caption controls and recover gates without replaying the write',
  );
  const overlay = fixture('linux');
  overlay.remaining = true;
  await overlay.chrome.start(true);
  assert(overlay.hidden, 'Other overlay keeps native video hidden');
  overlay.chrome.dispose();
  passed('Native geometry restoration preserves another overlay’s occlusion');
  const disposed = fixture('linux');
  const held = deferred();
  disposed.hold = held.promise;
  const starting = disposed.chrome.start(true);
  await until(() => disposed.hidden);
  disposed.chrome.dispose();
  held.resolve();
  await starting;
  assert(
    !disposed.chrome.custom &&
      disposed.order.join(',') === 'hide:true' &&
      !disposed.workspace.busy,
    'Disposed hide cannot mutate decorations or publish captions',
  );
  passed(
    'Disposal while native hide is pending suppresses decoration changes and late publication',
  );
  const group = fixture('windows');
  const modal = new WorkspaceModal(group.workspace, {
    ...group.effects,
    captureFocus: () => () => {},
  });
  await modal.open('actionMenu');
  assert(
    group.hidden && modal.identity === 'actionMenu',
    'Hide before group presentation',
  );
  const browser = mount(ActionBrowser, {
    target: host,
    props: {
      mode: 'actionMenu',
      group: 'File',
      context,
      cues: [],
      onaction: async () => {
        await modal.close();
      },
      oncue: async () => {},
      onclose: () => modal.close(),
    },
  });
  try {
    await tick();
    assert(
      host.querySelector('h2')?.textContent === 'File' &&
        host.querySelector('[data-action="openSubtitles"]') &&
        !host.querySelector('[data-action="undo"]'),
      'Group filters shared registry',
    );
    host
      .querySelector<HTMLButtonElement>('[data-action="openSubtitles"]')!
      .click();
    await until(() => !modal.active);
    assert(
      !group.hidden,
      'Action restores native visibility before continuation',
    );
    passed(
      'Grouped menus reuse the action registry and HideVideo modal close ordering',
    );
  } finally {
    await unmount(browser);
    modal.dispose();
    group.chrome.dispose();
  }
  document.querySelector('#result')!.textContent =
    'PASS: all 7 window-chrome checks';
}
run().catch((error) => {
  document.querySelector('#result')!.textContent = 'FAIL: ' + String(error);
  console.error(error);
});
