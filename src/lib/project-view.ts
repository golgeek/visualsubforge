import type { WorkspaceView } from './workspace-view.svelte';
import type { TimelineView } from './timeline-view.svelte';
import type { CueNavigation } from './cue-navigation.svelte';
import type { ProjectWorkflow } from './project-workflow.svelte';
export function projectView(
  view: WorkspaceView,
  timeline: TimelineView,
  navigation: CueNavigation,
): Pick<
  ConstructorParameters<typeof ProjectWorkflow>[2],
  'read' | 'beginRestore' | 'mediaOpened' | 'restoreOptions' | 'restoreScenes'
> {
  return {
    read: () => ({
      mediaPath: view.mediaPath,
      hardware: view.hardware,
      options: {
        waveformStart: timeline.start,
        waveformSpan: timeline.span,
        followPlayback: view.follow,
        followSubtitle: navigation.following !== 'off',
        showScenes: view.showScenes,
        timing: view.timingSettings,
      },
    }),
    beginRestore: () => {
      // Suspend analysis until the newly opened native session has a path.
      view.mediaPath = '';
      view.restoredScenes = null;
      timeline.reset();
    },
    mediaOpened: (path) => {
      view.mediaPath = path;
      // Rust resets the policy on media replacement; keep the projection on
      // defaults until project attachment accepts its saved policy.
      view.timingSettings = { ...view.preferences.timing };
    },
    restoreOptions: (options) => {
      ++view.qualityContext;
      view.follow = options.followPlayback;
      navigation.following = options.followSubtitle ? 'following' : 'off';
      view.showScenes = options.showScenes;
      view.timingSettings = { ...(options.timing ?? view.preferences.timing) };
      timeline.restore(options.waveformStart, options.waveformSpan);
    },
    restoreScenes: (session, generation) => {
      view.restoredScenes = { session, generation };
    },
  };
}
