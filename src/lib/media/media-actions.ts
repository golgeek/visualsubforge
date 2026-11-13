import type { Workspace } from '../workspace/workspace.svelte';
import type { MediaSession } from './media-session.svelte';
import type { ProjectWorkflow } from '../project/project-workflow.svelte';
import type { TimingGesture } from '../timing/timing-gesture.svelte';
import type { TimelineView } from '../timeline/timeline-view.svelte';
import { chooseFile, type MediaAction } from '../client';
import type { WorkspacePreferences } from '../settings/workspace-preferences.svelte';
import type { WorkspaceScenes } from '../timeline/workspace-scenes.svelte';

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
  preferences: WorkspacePreferences;
  scenes: WorkspaceScenes;
}

export async function runMediaAction(
  action: MediaWorkspaceAction,
  owners: Owners,
) {
  const {
    workspace,
    media,
    projects,
    gestures,
    timeline,
    preferences,
    scenes,
  } = owners;
  gestures.cancelHeld();
  workspace.notice = '';
  await workspace.task(async () => {
    switch (action) {
      case 'openMedia': {
        const path = await chooseFile('media', media.path || null);
        if (path) {
          timeline.reset(true);
          await commandMedia(owners, {
            kind: 'open',
            path,
            hardware: media.hardware,
          });
          media.path = path;
          await projects.detach();
          scenes.clearRestored();
          preferences.resetTiming();
        }
        break;
      }
      case 'closeMedia':
        await commandMedia(owners, { kind: 'close' });
        media.path = '';
        await projects.detach();
        preferences.resetTiming();
        scenes.clearRestored();
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

export async function seekMedia(
  owners: Pick<Owners, 'workspace' | 'media' | 'gestures'>,
  time: number,
) {
  const { workspace, media, gestures } = owners;
  if (!media.status.loaded || media.pending || workspace.busy) return;
  try {
    await commandMedia({ media, gestures }, { kind: 'seek', seconds: time });
  } catch (e) {
    workspace.error = String(e);
  }
}
export async function selectAudioTrack(
  owners: Pick<Owners, 'workspace' | 'media' | 'gestures' | 'timeline'>,
  id: number,
) {
  const { workspace, media, gestures, timeline } = owners;
  if (workspace.busy) return;
  await workspace.task(
    async () => {
      await commandMedia({ media, gestures }, { kind: 'track', id });
      timeline.invalidateTrack();
    },
    { clearError: false },
  );
}
