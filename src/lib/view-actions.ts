import type { WorkspaceView } from './workspace-view.svelte';
import type { CueNavigation } from './cue-navigation.svelte';
import type { Workspace } from './workspace.svelte';
import type { DocumentSession } from './document-session.svelte';
import type { MediaSession } from './media-session.svelte';
import type { TimingGesture } from './timing-gesture.svelte';
import { tick } from 'svelte';
import type { WorkspaceModal, AuthoringModal } from './workspace-modal.svelte';

export type ViewAction =
  | 'resync'
  | 'settings'
  | 'timingSettings'
  | 'translationReference'
  | 'resetLayout'
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
  layout: { topPct: number; editorPct: number; wavePct: number };
  view: {
    diagnostics: boolean;
    translationOpen: boolean;
    problemsOpen: boolean;
    showScenes: boolean;
    sceneControls:
      | {
          rescan: () => Promise<void>;
          cancel: () => Promise<void>;
          navigate: (backwards: boolean) => void;
        }
      | undefined;
  };
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
  } = owners;
  if (action === 'plugins' || action === 'convertSubtitles') {
    gestures.cancelHeld();
    await openAuthoring(action === 'plugins' ? 'plugins' : 'conversion');
    return;
  }
  if (action === 'showScenes') {
    view.showScenes = !view.showScenes;
    return;
  }
  if (action === 'scanScenes') {
    await view.sceneControls?.rescan();
    return;
  }
  if (action === 'cancelSceneScan') {
    await view.sceneControls?.cancel();
    return;
  }
  if (action === 'previousScene' || action === 'nextScene') {
    view.sceneControls?.navigate(action === 'previousScene');
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
        view.translationOpen = !view.translationOpen;
        if (view.translationOpen) view.problemsOpen = false;
        break;
      case 'resetLayout':
        owners.layout.topPct = 32;
        owners.layout.editorPct = 25;
        owners.layout.wavePct = 65;
        break;
      case 'diagnostics':
        if (!view.diagnostics) {
          await media.occlude(true);
          view.diagnostics = true;
        } else {
          view.diagnostics = false;
          await tick();
          await media.occlude(false);
        }
        break;
    }
  });
}

export function toggleProblems(view: WorkspaceView, navigation: CueNavigation) {
  view.problemsOpen = !view.problemsOpen;
  if (view.problemsOpen) view.translationOpen = false;
  if (!view.problemsOpen) navigation.pendingProblem = null;
}
export function closeProblems(view: WorkspaceView, navigation: CueNavigation) {
  view.problemsOpen = false;
  navigation.pendingProblem = null;
}
export function toggleTranslation(view: WorkspaceView) {
  view.translationOpen = !view.translationOpen;
  if (view.translationOpen) view.problemsOpen = false;
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
