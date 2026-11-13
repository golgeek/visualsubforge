import {
  editingShortcut,
  windowsShortcut,
  type EditingShortcut,
} from './shortcuts.ts';
import { clamp } from './timeline.ts';
import type { WorkspaceAction } from './client';
import type { Workspace } from './workspace.svelte';
import type { TimingGesture } from './timing-gesture.svelte';
import type { DocumentSession } from './document-session.svelte';
import type { MediaSession } from './media-session.svelte';
import type { TimelineView } from './timeline-view.svelte';
import type { CueNavigation } from './cue-navigation.svelte';
import type { QualitySession } from './quality-session.svelte';

type Key = Pick<
  KeyboardEvent,
  | 'key'
  | 'code'
  | 'ctrlKey'
  | 'metaKey'
  | 'altKey'
  | 'shiftKey'
  | 'repeat'
  | 'isComposing'
  | 'defaultPrevented'
>;
interface KeyContext {
  editing: boolean;
  control: boolean;
  menu: boolean;
  confirming: boolean;
  problems: boolean;
  windows: boolean;
  available: boolean;
  canAct: boolean;
  timing: boolean;
  paused: boolean;
}
export type WorkspaceKeyRoute =
  | { kind: 'text'; mode: 'find' | 'replace' }
  | { kind: 'problem'; previous: boolean }
  | { kind: 'editing'; action: EditingShortcut }
  | { kind: 'action'; action: WorkspaceAction }
  | { kind: 'held' }
  | { kind: 'consume' };

/** Pure routing policy; null leaves the key to the focused control/platform. */
export function routeWorkspaceKey(
  e: Key,
  context: KeyContext,
): WorkspaceKeyRoute | null {
  if (e.isComposing || e.defaultPrevented || context.menu || context.confirming)
    return null;
  if (
    (e.metaKey || e.ctrlKey) &&
    !e.altKey &&
    !e.repeat &&
    ((e.key.toLowerCase() === 'f' && !e.shiftKey) ||
      (e.key.toLowerCase() === 'h' &&
        ((e.ctrlKey && !e.shiftKey) || (e.metaKey && e.shiftKey))))
  )
    return {
      kind: 'text',
      mode: e.key.toLowerCase() === 'f' ? 'find' : 'replace',
    };
  if (
    context.problems &&
    e.key === 'F4' &&
    !e.metaKey &&
    !e.ctrlKey &&
    !e.altKey
  )
    return { kind: 'problem', previous: e.shiftKey };
  const shortcut = editingShortcut(e, context.editing);
  if (shortcut && !(shortcut === 'focusText' && context.control))
    return { kind: 'editing', action: shortcut };
  if (context.windows) {
    const action = windowsShortcut(e, context.editing);
    if (action)
      return action === 'openMedia' && !context.available
        ? { kind: 'consume' }
        : { kind: 'action', action };
  }
  if (
    e.code === 'Space' &&
    !context.editing &&
    !e.ctrlKey &&
    !e.metaKey &&
    !e.altKey &&
    !e.shiftKey &&
    !context.control
  ) {
    if (e.repeat || !context.canAct) return { kind: 'consume' };
    return context.timing && !context.paused
      ? { kind: 'held' }
      : { kind: 'action', action: 'play' };
  }
  return null;
}

interface KeyOwners {
  workspace: Workspace;
  gestures: TimingGesture;
  subtitles: DocumentSession;
  media: MediaSession;
  timeline: TimelineView;
  quality: QualitySession;
  navigation: CueNavigation;
  menu: boolean;
  waveformGesture: boolean;
  problemsOpen: boolean;
  windows: boolean;
  openText: (mode: 'find' | 'replace') => Promise<void>;
  requestProblem: (id: string) => void;
  focusWaveform: () => void;
  focusText: () => Promise<void>;
  revealSelection: () => Promise<void>;
  seek: (seconds: number) => Promise<void>;
}

export function workspaceKeydown(e: KeyboardEvent, owners: KeyOwners) {
  const { workspace, gestures, media, quality } = owners;
  const route = routeWorkspaceKey(e, {
    editing:
      e.target instanceof HTMLInputElement ||
      e.target instanceof HTMLTextAreaElement ||
      (e.target instanceof HTMLElement && e.target.isContentEditable),
    control:
      e.target instanceof HTMLButtonElement ||
      e.target instanceof HTMLSelectElement,
    menu: owners.menu,
    confirming: workspace.confirming,
    problems: owners.problemsOpen,
    windows: owners.windows,
    available: media.status.available,
    canAct: workspace.canAct,
    timing: gestures.mode,
    paused: media.status.paused,
  });
  if (!route) return;
  e.preventDefault();
  switch (route.kind) {
    case 'text':
      void owners.openText(route.mode);
      break;
    case 'problem': {
      const id = route.previous
        ? quality.view?.previousCue
        : quality.view?.nextCue;
      if (id && !quality.stale) owners.requestProblem(id);
      break;
    }
    case 'editing':
      void editingKey(route.action, owners);
      break;
    case 'action':
      void workspace.run(route.action);
      break;
    case 'held':
      gestures.beginHeld();
      break;
    case 'consume':
      break;
  }
}

export async function editingKey(action: EditingShortcut, owners: KeyOwners) {
  const {
    workspace,
    gestures,
    media,
    timeline,
    focusWaveform,
    focusText,
    seek,
  } = owners;
  if (!workspace.canAct || owners.menu || owners.waveformGesture) return;
  gestures.cancelHeld();
  if (action === 'play') {
    await workspace.run('play');
    return;
  }
  if (action === 'focusWaveform') {
    focusWaveform();
    return;
  }
  if (action === 'focusText') {
    await focusText();
    return;
  }
  if (action.startsWith('seek')) {
    if (media.status.positionSeconds !== null && media.status.loaded) {
      const amount = action.endsWith('Far') ? 5 : 1;
      await seek(
        clamp(
          media.status.positionSeconds +
            (action.includes('Back') ? -amount : amount),
          0,
          timeline.duration,
        ),
      );
    }
    return;
  }
  if (action === 'nextCue' || action === 'previousCue')
    await owners.navigation.editAdjacent(action);
}

export function workspaceKeyup(event: KeyboardEvent, gestures: TimingGesture) {
  if (event.code === 'Space' && gestures.held) {
    event.preventDefault();
    void gestures.releaseHeld();
  }
}
