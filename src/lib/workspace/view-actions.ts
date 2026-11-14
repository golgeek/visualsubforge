import type { WorkspaceView } from './workspace-view.svelte';
import type { WorkspaceScenes } from '../timeline/workspace-scenes.svelte';
import type { CueNavigation } from '../cues/cue-navigation.svelte';
import type { Workspace } from './workspace.svelte';
import type { DocumentSession } from '../document/document-session.svelte';
import type { MediaSession } from '../media/media-session.svelte';
import type { TimingGesture } from '../timing/timing-gesture.svelte';
import { tick } from 'svelte';
import type {
  WorkspaceModal,
  AuthoringModal,
} from '../dialogs/workspace-modal.svelte';

export type ViewAction =
  | 'resync'
  | 'settings'
  | 'timingSettings'
  | 'translationReference'
  | 'resetLayout'
  | 'arrangePanels'
  | 'diagnostics'
  | 'plugins'
  | 'convertSubtitles'
  | 'showScenes'
  | 'scanScenes'
  | 'cancelSceneScan'
  | 'previousScene'
  | 'nextScene';
interface Owners {
  workspace: Workspace;
  subtitles: DocumentSession;
  media: MediaSession;
  gestures: TimingGesture;
  workspaceModal: WorkspaceModal;
  openAuthoring: (kind: AuthoringModal) => Promise<void>;
  layout: { reset: () => Promise<void> };
  view: WorkspaceView;
  scenes: WorkspaceScenes;
  remainingOcclusion: () => boolean;
}

export async function runViewAction(action: ViewAction, owners: Owners) {
  const {
    workspace,
    subtitles,
    media,
    gestures,
    workspaceModal,
    openAuthoring,
    view,
    scenes,
  } = owners;
  if (action === 'plugins' || action === 'convertSubtitles') {
    gestures.cancelHeld();
    await openAuthoring(action === 'plugins' ? 'plugins' : 'conversion');
    return;
  }
  if (action === 'showScenes') {
    scenes.showScenes = !scenes.showScenes;
    return;
  }
  if (action === 'scanScenes') {
    await scenes.sceneControls?.rescan();
    return;
  }
  if (action === 'cancelSceneScan') {
    await scenes.sceneControls?.cancel();
    return;
  }
  if (action === 'previousScene' || action === 'nextScene') {
    scenes.sceneControls?.navigate(action === 'previousScene');
    return;
  }
  gestures.cancelHeld();
  workspace.notice = '';
  await workspace.task(async () => {
    switch (action) {
      case 'resync':
        if (!media.status.loaded || !subtitles.snapshot.cues.length) break;
        await workspaceModal.open('resync');
        break;
      case 'settings':
      case 'timingSettings':
        await workspaceModal.open('settings');
        break;
      case 'translationReference':
        view.toggleTranslation();
        break;
      case 'arrangePanels':
        if (
          media.status.loaded &&
          !(await media.command({ kind: 'pause', paused: true }))
        )
          break;
        if (!workspace.disposed) await workspaceModal.open('arrange');
        break;
      case 'resetLayout':
        await owners.layout.reset();
        break;
      case 'diagnostics':
        if (!view.diagnostics) {
          await media.occlude(true);
          view.diagnostics = true;
        } else {
          view.diagnostics = false;
          await tick();
          await media.occlude(owners.remainingOcclusion());
        }
        break;
    }
  });
}

export function setTimingMode(
  gestures: TimingGesture,
  navigation: CueNavigation,
  enabled: boolean,
) {
  gestures.cancelHeld();
  gestures.mode = enabled;
  navigation.focusWaveform();
}
