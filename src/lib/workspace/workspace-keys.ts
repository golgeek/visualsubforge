import {
  isNativeAction,
  matchesShortcut,
  type ActionId,
} from './action-registry.ts';
import { shortcutPreset } from './shortcut-settings.ts';
export type EditingShortcut =
  | 'setStart'
  | 'setEnd'
  | 'replay'
  | 'play'
  | 'seekBack'
  | 'seekForward'
  | 'seekBackFar'
  | 'seekForwardFar'
  | 'previousCue'
  | 'nextCue'
  | 'focusText'
  | 'focusWaveform';
import type { ShortcutPreferences } from '../client';
import { clamp, formatTime } from '../timeline/timeline.ts';
import type { WorkspaceAction } from '../client';
import type { Workspace } from './workspace.svelte';
import type { TimingGesture } from '../timing/timing-gesture.svelte';
import type { DocumentSession } from '../document/document-session.svelte';
import type { MediaSession } from '../media/media-session.svelte';
import type { TimelineView } from '../timeline/timeline-view.svelte';
import type { CueNavigation } from '../cues/cue-navigation.svelte';
import type { QualitySession } from '../quality/quality-session.svelte';

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
  cueText?: boolean;
  workspaceFocus?: boolean;
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
  | { kind: 'command'; action: ActionId }
  | { kind: 'browser'; mode: 'commands' | 'shortcuts' }
  | { kind: 'held' }
  | { kind: 'consume' };

/** Pure routing policy; null leaves the key to the focused control/platform. */
export function routeWorkspaceKey(
  e: Key,
  context: KeyContext,
  shortcuts: ShortcutPreferences = shortcutPreset('current'),
): WorkspaceKeyRoute | null {
  if (e.isComposing || e.defaultPrevented || context.menu || context.confirming)
    return null;
  const mode =
    (context.cueText ?? context.editing)
      ? 'text'
      : !context.editing && !context.control && (context.workspaceFocus ?? true)
        ? 'workspace'
        : 'control';
  const binding = shortcuts.bindings.find(
    (binding) =>
      (binding.mode === 'global' ||
        binding.mode === 'alternate' ||
        binding.mode === mode) &&
      matchesShortcut(binding.key, e),
  );
  if (!binding) return null;
  const action = binding.action as ActionId;
  if (
    action === 'play' &&
    binding.mode === 'workspace' &&
    binding.key === 'Space'
  ) {
    if (e.repeat || !context.canAct) return { kind: 'consume' };
    return context.timing && !context.paused
      ? { kind: 'held' }
      : { kind: 'action', action: 'play' };
  }
  if (e.repeat && action !== 'previousProblem' && action !== 'nextProblem')
    return null;
  if (action === 'commandPalette') return { kind: 'browser', mode: 'commands' };
  if (action === 'shortcutSheet' && context.editing && binding.key === '?')
    return null;
  if (action === 'shortcutSheet') return { kind: 'browser', mode: 'shortcuts' };
  if (action === 'find' || action === 'replace')
    return { kind: 'text', mode: action };
  if (action === 'previousProblem' || action === 'nextProblem')
    return context.problems
      ? { kind: 'problem', previous: action === 'previousProblem' }
      : null;
  if (
    [
      'focusText',
      'focusWaveform',
      'seekBack',
      'seekForward',
      'seekBackFar',
      'seekForwardFar',
      'previousCue',
      'nextCue',
      'setStart',
      'setEnd',
      'replay',
    ].includes(action)
  ) {
    if (action === 'focusText' && context.control) return null;
    return { kind: 'editing', action: action as EditingShortcut };
  }
  if (action === 'play' && binding.mode !== 'global')
    return { kind: 'editing', action: 'play' };
  if (isNativeAction(action)) {
    if (context.editing && (action === 'undo' || action === 'redo'))
      return null;
    // Primary native accelerators dispatch from menus on macOS/Linux. WebView2
    // needs the existing unconsumed-key fallback; scoped keys live in the DOM.
    if (binding.mode === 'global' && (!context.windows || e.metaKey))
      return null;
    return action === 'openMedia' && !context.available
      ? { kind: 'consume' }
      : { kind: 'action', action };
  }
  return { kind: 'command', action };
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
  shortcuts: ShortcutPreferences;
  runCommand: (id: ActionId) => Promise<void>;
  openCommands: (mode: 'commands' | 'shortcuts') => Promise<void>;
  openText: (mode: 'find' | 'replace') => Promise<void>;
  requestProblem: (id: string) => void;
  focusWaveform: () => void;
  focusText: () => Promise<void>;
  revealSelection: () => Promise<void>;
  seek: (seconds: number) => Promise<void>;
}

export function workspaceKeydown(e: KeyboardEvent, owners: KeyOwners) {
  const { workspace, gestures, media, quality } = owners;
  const route = routeWorkspaceKey(
    e,
    {
      cueText:
        e.target instanceof HTMLTextAreaElement &&
        !!e.target.closest('.editor'),
      workspaceFocus:
        e.target instanceof Element && !!e.target.closest('.cue-table, .wave'),
      editing:
        e.target instanceof HTMLInputElement ||
        e.target instanceof HTMLTextAreaElement ||
        (e.target instanceof HTMLElement && e.target.isContentEditable),
      control:
        (e.target instanceof HTMLButtonElement &&
          !e.target.matches('.cue-row')) ||
        e.target instanceof HTMLSelectElement,
      menu: owners.menu,
      confirming: workspace.confirming,
      problems: owners.problemsOpen,
      windows: owners.windows,
      available: media.status.available,
      canAct: workspace.canAct,
      timing: gestures.mode,
      paused: media.status.paused,
    },
    owners.shortcuts,
  );
  if (!route) return;
  e.preventDefault();
  switch (route.kind) {
    case 'browser':
      void owners.openCommands(route.mode);
      break;
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
    case 'command':
      void owners.runCommand(route.action);
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
  if (action === 'setStart' || action === 'setEnd') {
    const { subtitles } = owners;
    const selected = subtitles.selected;
    const generation = subtitles.snapshot.generation;
    const status = media.status;
    if (
      !status.loaded ||
      media.pending ||
      status.seeking ||
      status.positionSeconds === null ||
      !subtitles.cue
    )
      return;
    const step = subtitles.snapshot.format?.timeStepMs ?? 1;
    const position = Math.round((status.positionSeconds * 1000) / step) * step;
    await workspace.task(async () => {
      if (
        !(await subtitles.apply()) ||
        workspace.disposed ||
        subtitles.selected !== selected ||
        subtitles.snapshot.generation !== generation ||
        media.status.session !== status.session
      )
        return;
      const cue = subtitles.cue;
      if (!cue) return;
      if (
        position < 0 ||
        (action === 'setStart'
          ? position >= cue.endMs
          : position <= cue.startMs)
      )
        throw new Error(
          'The start must be before the end. Move the playhead and try again.',
        );
      if (action === 'setStart') subtitles.start = formatTime(position);
      else subtitles.end = formatTime(position);
      await subtitles.apply(null);
    });
    return;
  }
  if (action === 'replay') {
    const status = media.status;
    if (!status.loaded || media.pending || status.positionSeconds === null)
      return;
    const seconds = Math.max(0, status.positionSeconds - 2);
    await workspace.task(async () => {
      if (
        !(await media.command({ kind: 'seek', seconds })) ||
        workspace.disposed ||
        media.status.session !== status.session
      )
        return;
      if (media.status.paused)
        await media.command({ kind: 'pause', paused: false });
    });
    return;
  }
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
