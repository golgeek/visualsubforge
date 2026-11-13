import { workspaceActions, updateMenu } from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
import type { MediaSession } from '../media/media-session.svelte';
import type { Workspace } from './workspace.svelte';
import type { WorkspaceView } from './workspace-view.svelte';
import type { WorkspaceScenes } from '../timeline/workspace-scenes.svelte';
import type { TimelineView } from '../timeline/timeline-view.svelte';
import type { CueNavigation } from '../cues/cue-navigation.svelte';
import type { CueEditing } from '../cues/cue-editing.svelte';
export class WorkspaceMenus {
  private alive = true;
  private lastMenu = '';
  private unlisten = () => {};
  constructor(
    private workspace: Workspace,
    private subtitles: DocumentSession,
    private media: MediaSession,
    private view: WorkspaceView,
    private scenes: WorkspaceScenes,
    private timeline: TimelineView,
    private navigation: CueNavigation,
    private cueEditing: CueEditing,
  ) {
    $effect(() => {
      if (!this.alive) return;
      const state = {
        diagnostics: this.view.diagnostics,
        showScenes: this.scenes.showScenes,
        scenesRunning: this.scenes.sceneState.running,
        scenesPending: this.scenes.sceneState.pending,
        previousScene: this.scenes.sceneState.previous,
        nextScene: this.scenes.sceneState.next,
        available: this.media.status.available,
        media: this.media.status.loaded,
        document: this.subtitles.snapshot.loaded,
        convert:
          this.subtitles.snapshot.loaded &&
          this.subtitles.snapshot.format?.name !== 'vtt',
        resync:
          this.media.status.loaded && this.subtitles.snapshot.cues.length > 0,
        undo:
          this.subtitles.snapshot.canUndo ||
          this.subtitles.dirty ||
          this.navigation.textFocused,
        redo:
          this.subtitles.snapshot.canRedo ||
          this.subtitles.dirty ||
          this.navigation.textFocused,
        range:
          !!this.timeline.range &&
          this.timeline.range[1] > this.timeline.range[0],
        busy: !this.workspace.canAct || this.timeline.interacting,
        createCue: !!this.cueEditing.operations.find(
          (op) => op.operation === 'createCue',
        )?.plan,
        deleteCue: !!this.cueEditing.operations.find(
          (op) => op.operation === 'deleteCue',
        )?.plan,
        splitCue: !!this.cueEditing.operations.find(
          (op) => op.operation === 'splitCue',
        )?.plan,
        mergePrevious: !!this.cueEditing.operations.find(
          (op) => op.operation === 'mergePrevious',
        )?.plan,
        mergeNext: !!this.cueEditing.operations.find(
          (op) => op.operation === 'mergeNext',
        )?.plan,
      };
      const key = JSON.stringify(state);
      if (key === this.lastMenu) return;
      this.lastMenu = key;
      void updateMenu(state).catch((e) => {
        if (this.alive) this.workspace.error = String(e);
      });
    });
  }
  startListening() {
    void workspaceActions((action) => {
      if (this.alive) void this.workspace.run(action);
    })
      .then((stop) => {
        if (this.alive) this.unlisten = stop;
        else stop();
      })
      .catch((error) => {
        if (this.alive) this.workspace.error = String(error);
      });
  }
  dispose() {
    this.alive = false;
    this.unlisten();
  }
}
