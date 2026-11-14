import descriptions from './action-descriptions.json' with { type: 'json' };
import type { WorkspaceAction } from '../client';

export type ActionId = keyof typeof descriptions;
export type ActionGroup =
  'File' | 'Edit' | 'Cue' | 'View' | 'Playback' | 'Tools' | 'Review' | 'Help';
export interface ActionDescription {
  label: string;
  group: string;
  native: boolean;
  shortcut: string;
  aliases: string[];
  alternateShortcuts: string[];
  macShortcut: string | null;
}
// A Rust event added without a description fails TypeScript checking.
const nativeCoverage: Record<WorkspaceAction, ActionDescription> = descriptions;
void nativeCoverage;
export const actions = Object.entries(descriptions).map(
  ([id, description]) => ({
    id: id as ActionId,
    ...description,
  }),
);
export function actionDescription(id: ActionId): ActionDescription {
  return descriptions[id];
}
export function isNativeAction(id: ActionId): id is WorkspaceAction {
  return descriptions[id].native;
}

export interface ActionContext {
  ready: boolean;
  busy: boolean;
  document: boolean;
  media: boolean;
  available: boolean;
  cue: boolean;
  cues: boolean;
  draft: boolean;
  styled: boolean;
  webvtt: boolean;
  undo: boolean;
  redo: boolean;
  range: boolean;
  scenesRunning: boolean;
  scenesPending: boolean;
  previousScene: boolean;
  nextScene: boolean;
  problems: boolean;
  previousProblem: boolean;
  nextProblem: boolean;
  operations: Partial<Record<ActionId, string | null>>;
}
/** Presentation eligibility only. Operation owners recheck before dispatch/commit. */
export function unavailable(id: ActionId, state: ActionContext): string | null {
  if (!state.ready) return 'Workspace is starting';
  if (state.busy) return 'Finish the current operation';
  if (id in state.operations) return state.operations[id] ?? null;
  switch (id) {
    case 'openMedia':
      return state.available ? null : 'Video playback is unavailable';
    case 'saveProject':
    case 'saveProjectAs':
      return state.document || state.media ? null : 'Open subtitles or media';
    case 'closeMedia':
    case 'play':
    case 'previousFrame':
    case 'nextFrame':
    case 'zoomIn':
    case 'zoomOut':
    case 'fitWaveform':
    case 'clearLoop':
    case 'seekBack':
    case 'seekForward':
    case 'seekBackFar':
    case 'seekForwardFar':
      return state.media ? null : 'Open media first';
    case 'playRange':
      return !state.media
        ? 'Open media first'
        : state.range
          ? null
          : 'Select a time range';
    case 'resync':
      return !state.media
        ? 'Open media first'
        : state.cues
          ? null
          : 'Open subtitles with cues';
    case 'scanScenes':
      return !state.media
        ? 'Open media first'
        : state.scenesPending || state.scenesRunning
          ? 'A shot-change scan is already running'
          : null;
    case 'cancelSceneScan':
      return state.scenesRunning && !state.scenesPending
        ? null
        : 'No shot-change scan is running';
    case 'previousScene':
    case 'nextScene':
      return state.media &&
        (id === 'previousScene' ? state.previousScene : state.nextScene)
        ? null
        : 'Needs a shot-change scan and a nearby cut';
    case 'undo':
      return state.draft
        ? 'Apply or revert the current edit'
        : state.undo
          ? null
          : 'Nothing to undo';
    case 'redo':
      return state.draft
        ? 'Apply or revert the current edit'
        : state.redo
          ? null
          : 'Nothing to redo';
    case 'apply':
    case 'revertDraft':
      return state.draft ? null : 'No unapplied edit';
    case 'focusText':
    case 'previousCue':
    case 'nextCue':
      return state.cue ? null : 'Select a cue';
    case 'styles':
    case 'fonts':
      return state.styled ? null : 'Open ASS or SSA subtitles';
    case 'convertSubtitles':
      return !state.document
        ? 'Open subtitles first'
        : state.webvtt
          ? 'WebVTT export is not available'
          : null;
    case 'fixGaps':
    case 'fixReadingSpeed':
    case 'shiftCues':
      return state.cues ? null : 'Open subtitles with cues';
    case 'snapShots':
      return !state.cues
        ? 'Open subtitles with cues'
        : state.media &&
            !state.scenesRunning &&
            !state.scenesPending &&
            (state.previousScene || state.nextScene)
          ? null
          : 'Needs a completed shot-change scan';
    case 'previousProblem':
    case 'nextProblem':
      return state.problems &&
        (id === 'previousProblem' ? state.previousProblem : state.nextProblem)
        ? null
        : 'Open Problems and select a finding';
    case 'save':
    case 'saveAs':
    case 'closeSubtitles':
    case 'plugins':
    case 'statistics':
    case 'find':
    case 'replace':
    case 'problems':
      return state.document ? null : 'Open subtitles first';
    default:
      return null;
  }
}

export type ShortcutKey = Pick<
  KeyboardEvent,
  'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'
>;
const keyNames: Record<string, string> = {
  Return: 'Enter',
  Left: 'ArrowLeft',
  Right: 'ArrowRight',
  Up: 'ArrowUp',
  Down: 'ArrowDown',
  Space: ' ',
};
export function matchesShortcut(shortcut: string, event: ShortcutKey): boolean {
  if (!shortcut) return false;
  const parts = shortcut.split('+');
  const key = parts.pop()!;
  const mod = parts.includes('CmdOrCtrl');
  return (
    (keyNames[key] ?? key).toLowerCase() === event.key.toLowerCase() &&
    (mod
      ? event.ctrlKey || event.metaKey
      : event.ctrlKey === parts.includes('Ctrl') &&
        event.metaKey === parts.includes('Meta')) &&
    event.altKey === parts.includes('Alt') &&
    (key === '?' || event.shiftKey === parts.includes('Shift'))
  );
}
export function shortcutLabel(shortcut: string, mac: boolean): string {
  if (!shortcut) return '';
  const glyphs: Record<string, string> = mac
    ? {
        CmdOrCtrl: '⌘',
        Meta: '⌘',
        Ctrl: '⌃',
        Alt: '⌥',
        Shift: '⇧',
        Return: '↵',
        Enter: '↵',
        Escape: 'Esc',
        Left: '←',
        Right: '→',
        Up: '↑',
        Down: '↓',
        Space: 'Space',
      }
    : {
        CmdOrCtrl: 'Ctrl',
        Meta: 'Meta',
        Return: 'Enter',
        Left: '←',
        Right: '→',
        Up: '↑',
        Down: '↓',
      };
  return shortcut
    .split('+')
    .map((part) => glyphs[part] ?? part)
    .join(mac ? '' : '+');
}
