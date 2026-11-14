import type { WorkspacePreferences } from '../settings/workspace-preferences.svelte';
import type { WorkspaceScenes } from '../timeline/workspace-scenes.svelte';
import type { MediaSession } from '../media/media-session.svelte';
import type { TimelineView } from '../timeline/timeline-view.svelte';
import type { CueNavigation } from '../cues/cue-navigation.svelte';
import type { WorkspaceLayout } from '../layout/workspace-layout.svelte';
import type { ProjectOptions } from '../client';
interface Owners {
  layout?: WorkspaceLayout;
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
  layout,
}: Owners) {
  return {
    mediaPath: media.path,
    hardware: media.hardware,
    options: {
      layout: layout?.projectSnapshot ?? null,
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
export async function restoreProjectOptions(
  { preferences, scenes, timeline, navigation, layout }: Owners,
  options: ProjectOptions,
  check: () => void = () => {},
) {
  await layout?.restoreProject(options.layout, check);
  check();
  timeline.follow = options.followPlayback;
  navigation.following = options.followSubtitle ? 'following' : 'off';
  scenes.showScenes = options.showScenes;
  preferences.restoreTiming(options.timing);
  timeline.restore(options.waveformStart, options.waveformSpan);
}
