import type { WorkspacePreferences } from './workspace-preferences.svelte';
import type { WorkspaceScenes } from './workspace-scenes.svelte';
import type { MediaSession } from './media-session.svelte';
import type { TimelineView } from './timeline-view.svelte';
import type { CueNavigation } from './cue-navigation.svelte';
import type { ProjectOptions } from './client';
interface Owners {
  media: MediaSession;
  preferences: WorkspacePreferences;
  scenes: WorkspaceScenes;
  timeline: TimelineView;
  navigation: CueNavigation;
}
export function readProjectView({
  media,
  preferences,
  scenes,
  timeline,
  navigation,
}: Owners) {
  return {
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
  };
}
export function beginProjectRestore({ media, scenes, timeline }: Owners) {
  // Suspend analysis until the newly opened native session has a path.
  media.path = '';
  scenes.clearRestored();
  timeline.reset();
}
export function projectMediaOpened(
  { media, preferences }: Owners,
  path: string,
) {
  media.path = path;
  // Rust resets timing on media replacement; project attachment restores it later.
  preferences.resetTiming();
}
export function restoreProjectOptions(
  { preferences, scenes, timeline, navigation }: Owners,
  options: ProjectOptions,
) {
  timeline.follow = options.followPlayback;
  navigation.following = options.followSubtitle ? 'following' : 'off';
  scenes.showScenes = options.showScenes;
  preferences.restoreTiming(options.timing);
  timeline.restore(options.waveformStart, options.waveformSpan);
}
