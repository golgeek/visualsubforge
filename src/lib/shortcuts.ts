import type { WorkspaceAction } from './client';

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
  if (e.key === 'Enter' && !e.shiftKey) return e.altKey ? 'play' : 'apply';
  // Ctrl+Alt can represent AltGr; never reinterpret it as a file/edit command.
  if (e.altKey) return null;
  switch (e.key.toLowerCase()) {
    case ',':
      return e.shiftKey ? null : 'settings';
    case 'n':
      return e.shiftKey ? 'createCue' : null;
    case 'd':
      return e.shiftKey ? 'deleteCue' : null;
    case 'k':
      return e.shiftKey ? 'splitCue' : null;
    case 'h':
      return e.shiftKey ? 'mergePrevious' : null;
    case 'j':
      return e.shiftKey ? 'mergeNext' : null;
    case 'o':
      return e.shiftKey ? 'openSubtitles' : 'openMedia';
    case 's':
      return e.shiftKey ? 'saveAs' : 'save';
    case 'z':
      return editingText ? null : e.shiftKey ? 'redo' : 'undo';
    default:
      return null;
  }
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
  if (e.key === 'Escape' && !e.altKey && !e.shiftKey) return 'focusWaveform';
  if (!editingText && e.key === 'Enter' && !e.altKey && !e.shiftKey)
    return 'focusText';
  if (!e.altKey) return null;
  switch (e.key) {
    case ' ':
      return e.shiftKey ? null : 'play';
    case 'ArrowLeft':
      return e.shiftKey ? 'seekBackFar' : 'seekBack';
    case 'ArrowRight':
      return e.shiftKey ? 'seekForwardFar' : 'seekForward';
    case 'ArrowUp':
      return e.shiftKey ? null : 'previousCue';
    case 'ArrowDown':
      return e.shiftKey ? null : 'nextCue';
    default:
      return null;
  }
}
