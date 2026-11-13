import { updateMenu } from './client';
import type { DocumentSession } from './document-session.svelte';
import type { MediaSession } from './media-session.svelte';
import type { Workspace } from './workspace.svelte';
import type { WorkspaceView } from './workspace-view.svelte';
import type { TimelineView } from './timeline-view.svelte';
import type { CueNavigation } from './cue-navigation.svelte';
import type { createCueEditing } from './cue-editing.svelte';
export function connectWorkspaceMenus(
  workspace: Workspace,
  subtitles: DocumentSession,
  media: MediaSession,
  view: WorkspaceView,
  timeline: TimelineView,
  navigation: CueNavigation,
  cueEditing: ReturnType<typeof createCueEditing>,
) {
  const doc = $derived(subtitles.snapshot);
  const draftDirty = $derived(subtitles.dirty);
  const status = $derived(media.status);
  let lastMenu = '';
  $effect(() => {
    const state = {
      diagnostics: view.diagnostics,
      showScenes: view.showScenes,
      scenesRunning: view.sceneState.running,
      scenesPending: view.sceneState.pending,
      previousScene: view.sceneState.previous,
      nextScene: view.sceneState.next,
      available: status.available,
      media: status.loaded,
      document: doc.loaded,
      convert: doc.loaded && doc.format?.name !== 'vtt',
      resync: status.loaded && doc.cues.length > 0,
      undo: doc.canUndo || draftDirty || navigation.textFocused,
      redo: doc.canRedo || draftDirty || navigation.textFocused,
      range: !!timeline.range && timeline.range[1] > timeline.range[0],
      busy: !workspace.canAct || view.waveformGesture,
      createCue: !!cueEditing.operations.find(
        (op) => op.operation === 'createCue',
      )?.plan,
      deleteCue: !!cueEditing.operations.find(
        (op) => op.operation === 'deleteCue',
      )?.plan,
      splitCue: !!cueEditing.operations.find(
        (op) => op.operation === 'splitCue',
      )?.plan,
      mergePrevious: !!cueEditing.operations.find(
        (op) => op.operation === 'mergePrevious',
      )?.plan,
      mergeNext: !!cueEditing.operations.find(
        (op) => op.operation === 'mergeNext',
      )?.plan,
    };
    const key = JSON.stringify(state);
    if (key === lastMenu) return;
    lastMenu = key;
    void updateMenu(state).catch((e) => (workspace.error = String(e)));
  });
}
