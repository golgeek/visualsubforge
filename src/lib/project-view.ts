import type { WorkspacePreferences } from './workspace-preferences.svelte';
import type { WorkspaceScenes } from './workspace-scenes.svelte';
import type { MediaSession } from './media-session.svelte';
import type { TimelineView } from './timeline-view.svelte';
import type { CueNavigation } from './cue-navigation.svelte';
import type { ProjectWorkflow } from './project-workflow.svelte';
export function projectView(
  media: MediaSession,
  preferences: WorkspacePreferences,
  scenes: WorkspaceScenes,
  timeline: TimelineView,
  navigation: CueNavigation,
): Pick<
  ConstructorParameters<typeof ProjectWorkflow>[2],
  'read' | 'beginRestore' | 'mediaOpened' | 'restoreOptions' | 'restoreScenes'
> {
  return {
    read: () => ({
      mediaPath: media.path,
      hardware: media.hardware,
      options: {
        waveformStart: timeline.start,
        waveformSpan: timeline.span,
        followPlayback: timeline.follow,
        followSubtitle: navigation.following !== 'off',
        showScenes: scenes.showScenes,
        timing: preferences.timingSettings,
      },
    }),
    beginRestore: () => {
      // Suspend analysis until the newly opened native session has a path.
      media.path = '';
      scenes.clearRestored();
      timeline.reset();
    },
    mediaOpened: (path) => {
      media.path = path;
      // Rust resets the policy on media replacement; keep the projection on
      // defaults until project attachment accepts its saved policy.
      preferences.resetTiming();
    },
    restoreOptions: (options) => {
      timeline.follow = options.followPlayback;
      navigation.following = options.followSubtitle ? 'following' : 'off';
      scenes.showScenes = options.showScenes;
      preferences.restoreTiming(options.timing);
      timeline.restore(options.waveformStart, options.waveformSpan);
    },
    restoreScenes: (session, generation) => {
      scenes.restore(session, generation);
    },
  };
}
