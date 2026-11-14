import presets from './shortcut-presets.json' with { type: 'json' };
import descriptions from './action-descriptions.json' with { type: 'json' };
import type {
  ShortcutPreferences,
  ShortcutBinding,
  ShortcutMode,
} from '../client';
import {
  matchesShortcut,
  type ActionId,
  type ShortcutKey,
} from './action-registry.ts';

export const shortcutModes: { id: ShortcutMode; label: string }[] = [
  { id: 'global', label: 'Everywhere' },
  { id: 'alternate', label: 'Everywhere (alternate)' },
  { id: 'workspace', label: 'List / waveform' },
  { id: 'text', label: 'Cue text' },
];
export function shortcutPreset(
  preset: Exclude<ShortcutPreferences['preset'], 'custom'>,
): ShortcutPreferences {
  return {
    preset,
    bindings: structuredClone(presets[preset]) as ShortcutBinding[],
  };
}
export const canonicalShortcut = (key: string) =>
  key
    .split('+')
    .map((part) => (part === 'Enter' ? 'Return' : part))
    .join('+');
const globalMode = (mode: ShortcutMode) =>
  mode === 'global' || mode === 'alternate';
export function shortcutConflict(
  bindings: readonly ShortcutBinding[],
  binding: ShortcutBinding,
  index: number,
): ShortcutBinding | undefined {
  if (!binding.key) return undefined;
  return bindings.find(
    (other, position) =>
      position !== index &&
      other.key &&
      (other.mode === binding.mode ||
        globalMode(other.mode) ||
        globalMode(binding.mode)) &&
      canonicalShortcut(other.key) === canonicalShortcut(binding.key),
  );
}
export function shortcutError(settings: ShortcutPreferences): string | null {
  const slots = new Set<string>();
  for (const [index, binding] of settings.bindings.entries()) {
    const slot = `${binding.action}/${binding.mode}`;
    if (!(binding.action in descriptions) || slots.has(slot))
      return 'Unknown action or duplicate focus mode';
    slots.add(slot);
    if (!binding.key) continue;
    const parts = binding.key.split('+');
    const key = parts.pop()!;
    const modifiers = ['CmdOrCtrl', 'Alt', 'Shift'].filter((m) =>
      parts.includes(m),
    );
    if (
      parts.join('+') !== modifiers.join('+') ||
      binding.key.length > 48 ||
      !/^(?:[A-Z0-9[\],./?=-]|F(?:[1-9]|1\d|2[0-4])|Return|Enter|Escape|Space|Tab|Left|Right|Up|Down|Backspace|Delete)$/.test(
        key,
      ) ||
      (key === '?' && parts.includes('Shift'))
    )
      return 'Use CmdOrCtrl, Alt, Shift and a supported key, in that order';
    if (
      parts.includes('CmdOrCtrl') &&
      parts.includes('Alt') &&
      key.length === 1
    )
      return 'Ctrl+Alt character keys are reserved for AltGr text input';
    const modified = parts.includes('CmdOrCtrl') || parts.includes('Alt');
    if (
      globalMode(binding.mode) &&
      !modified &&
      !/^F\d+$/.test(key) &&
      !(key === 'Escape' && binding.action === 'focusWaveform') &&
      !(key === '?' && binding.action === 'shortcutSheet')
    )
      return 'Everywhere shortcuts need a command/Alt modifier or a function key';
    if (
      binding.mode === 'text' &&
      !modified &&
      !/^F\d+$/.test(key) &&
      key !== 'Tab'
    )
      return 'Cue-text shortcuts must preserve ordinary typing';
    const reserved = [
      'CmdOrCtrl+A',
      'CmdOrCtrl+B',
      'CmdOrCtrl+C',
      'CmdOrCtrl+I',
      'CmdOrCtrl+U',
      'CmdOrCtrl+V',
      'CmdOrCtrl+X',
      'CmdOrCtrl+H',
      'CmdOrCtrl+Space',
      'CmdOrCtrl+Tab',
      'Alt+Tab',
    ];
    if (
      reserved.includes(binding.key) ||
      (binding.key === 'Escape' && binding.action !== 'focusWaveform') ||
      (binding.key === 'CmdOrCtrl+Q' && binding.action !== 'quit')
    )
      return 'This shortcut is reserved for text editing or the operating system';
    const conflict = shortcutConflict(settings.bindings, binding, index);
    if (conflict)
      return `${descriptions[binding.action as ActionId].label} conflicts with ${descriptions[conflict.action as ActionId].label} in an overlapping focus mode`;
  }
  for (const [action, key] of [
    ['quit', 'CmdOrCtrl+Q'],
    ['focusWaveform', 'Escape'],
  ])
    if (
      !settings.bindings.some(
        (b) => b.action === action && b.mode === 'global' && b.key === key,
      )
    )
      return `${action} keeps its reserved ${key} binding`;
  return null;
}
export function shortcutFromEvent(event: ShortcutKey): string | null {
  if (
    ['Control', 'Meta', 'Alt', 'Shift', 'Dead', 'Unidentified'].includes(
      event.key,
    )
  )
    return null;
  const names: Record<string, string> = {
    ' ': 'Space',
    Enter: 'Return',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    ArrowUp: 'Up',
    ArrowDown: 'Down',
  };
  const key =
    names[event.key] ??
    (event.key.length === 1 ? event.key.toUpperCase() : event.key);
  return [
    event.ctrlKey || event.metaKey ? 'CmdOrCtrl' : '',
    event.altKey ? 'Alt' : '',
    event.shiftKey && key !== '?' ? 'Shift' : '',
    key,
  ]
    .filter(Boolean)
    .join('+');
}
export function bindingAction(
  settings: ShortcutPreferences,
  event: ShortcutKey,
  mode: 'workspace' | 'text' | 'control',
): ActionId | null {
  if (
    mode === 'text' &&
    event.key === '?' &&
    !event.ctrlKey &&
    !event.metaKey &&
    !event.altKey
  )
    return null;
  const binding = settings.bindings.find(
    (b) =>
      (globalMode(b.mode) || b.mode === mode) && matchesShortcut(b.key, event),
  );
  return (binding?.action as ActionId) ?? null;
}
