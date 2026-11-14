import {
  actionDescription,
  matchesShortcut,
} from '../workspace/action-registry.ts';
import type { WorkspaceAction } from '../client';

type Key = Pick<
  KeyboardEvent,
  | 'key'
  | 'ctrlKey'
  | 'metaKey'
  | 'altKey'
  | 'shiftKey'
  | 'repeat'
  | 'isComposing'
  | 'defaultPrevented'
>;

// WebView2 delivers these keys without activating the native menu in the
// tested Windows runtime. Only handle unconsumed keys, using the same actions.
export function windowsShortcut(
  e: Key,
  editingText: boolean,
): WorkspaceAction | null {
  if (
    e.defaultPrevented ||
    e.repeat ||
    e.isComposing ||
    !e.ctrlKey ||
    e.metaKey
  )
    return null;
  // Preserve the existing WebView2 fallback set and text-history boundary.
  const fallback: WorkspaceAction[] = [
    'settings',
    'createCue',
    'deleteCue',
    'splitCue',
    'mergePrevious',
    'mergeNext',
    'openMedia',
    'openSubtitles',
    'save',
    'saveAs',
    'undo',
    'redo',
    'apply',
    'play',
  ];
  for (const action of fallback) {
    if (editingText && (action === 'undo' || action === 'redo')) continue;
    if (matchesShortcut(actionDescription(action).shortcut, e)) return action;
  }
  return null;
}

export type EditingShortcut =
  | 'play'
  | 'seekBack'
  | 'seekForward'
  | 'seekBackFar'
  | 'seekForwardFar'
  | 'previousCue'
  | 'nextCue'
  | 'focusText'
  | 'focusWaveform';

// Alt shortcuts remain available while typing. Ctrl+Alt/AltGr and composition
// are excluded so international text input keeps its normal meaning.
export function editingShortcut(
  e: Key,
  editingText: boolean,
): EditingShortcut | null {
  if (e.defaultPrevented || e.isComposing || e.repeat || e.ctrlKey || e.metaKey)
    return null;
  const candidates: EditingShortcut[] = [
    'focusWaveform',
    'focusText',
    'seekBack',
    'seekForward',
    'seekBackFar',
    'seekForwardFar',
    'previousCue',
    'nextCue',
  ];
  for (const action of candidates) {
    if (action === 'focusText' && editingText) continue;
    if (matchesShortcut(actionDescription(action).shortcut, e)) return action;
  }
  return matchesShortcut(actionDescription('play').alternateShortcuts[0]!, e)
    ? 'play'
    : null;
}
