import type { Workspace } from './workspace.svelte';
import type { MediaSession } from './media-session.svelte';
import type { ProjectWorkflow } from './project-workflow.svelte';
import type { TimingGesture } from './timing-gesture.svelte';
import type { TimelineView } from './timeline-view.svelte';
import { chooseFile, type Preferences, type MediaAction } from './client';
import type { TimingSettings } from './magnetic-timing';

export type MediaWorkspaceAction =
  | 'openMedia'
  | 'closeMedia'
  | 'play'
  | 'previousFrame'
  | 'nextFrame'
  | 'playRange'
  | 'clearLoop';
interface Owners {
  workspace: Workspace;
  media: MediaSession;
  projects: ProjectWorkflow;
  gestures: TimingGesture;
  timeline: TimelineView;
  view: {
    mediaPath: string;
    hardware: boolean;
    preferences: Preferences;
    timingSettings: TimingSettings;
    restoredScenes: { session: number; generation: number } | null;
  };
}

export async function runMediaAction(
  action: MediaWorkspaceAction,
  owners: Owners,
) {
  const { workspace, media, projects, gestures, timeline, view } = owners;
  gestures.cancelHeld();
  workspace.notice = '';
  await workspace.task(async () => {
    switch (action) {
      case 'openMedia': {
        const path = await chooseFile('media', view.mediaPath || null);
        if (path) {
          timeline.reset(true);
          await commandMedia(owners, {
            kind: 'open',
            path,
            hardware: view.hardware,
          });
          view.mediaPath = path;
          await projects.detach();
          view.restoredScenes = null;
          view.timingSettings = { ...view.preferences.timing };
        }
        break;
      }
      case 'closeMedia':
        await commandMedia(owners, { kind: 'close' });
        view.mediaPath = '';
        await projects.detach();
        view.timingSettings = { ...view.preferences.timing };
        view.restoredScenes = null;
        timeline.reset();
        break;
      case 'play':
        if (media.status.loaded)
          await commandMedia(owners, {
            kind: 'pause',
            paused: !media.status.paused,
          });
        break;
      case 'previousFrame':
      case 'nextFrame':
        if (media.status.loaded)
          await commandMedia(owners, {
            kind: 'step',
            backwards: action === 'previousFrame',
          });
        break;
      case 'playRange':
        if (
          media.status.loaded &&
          timeline.range &&
          timeline.range[1] > timeline.range[0]
        ) {
          await commandMedia(owners, {
            kind: 'loop',
            start: timeline.range[0],
            end: timeline.range[1],
          });
          await commandMedia(owners, {
            kind: 'seek',
            seconds: timeline.range[0],
          });
          await commandMedia(owners, { kind: 'pause', paused: false });
        }
        break;
      case 'clearLoop':
        if (media.status.loaded)
          await commandMedia(owners, { kind: 'clearLoop' });
        break;
    }
  });
}

// Used by workspace actions and direct seek/track interactions alike.
export function commandMedia(
  owners: Pick<Owners, 'media' | 'gestures'>,
  action: MediaAction,
) {
  owners.gestures.cancelHeld();
  return owners.media.command(action);
}
