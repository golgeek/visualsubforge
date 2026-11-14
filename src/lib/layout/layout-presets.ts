import type {
  visualsubforge_preferences_layout_LayoutPreferences as LayoutPreferences,
  visualsubforge_preferences_layout_PaneSizes as PaneSizes,
  visualsubforge_preferences_layout_Preset as Preset,
} from '../generated/ipc';

export type LayoutPreset = Preset;
export type LayoutSlot = 'preview' | 'transport' | 'cues' | 'editor';
export interface LayoutDescription {
  label: string;
  rows: readonly LayoutSlot[];
  dock: 'review' | 'translation' | null;
  defaults: PaneSizes;
}
const timing = { topPct: 13, editorPct: 16, wavePct: 65 };
export const layoutPresets: Record<LayoutPreset, LayoutDescription> = {
  timing: {
    label: 'Timing',
    rows: ['preview', 'transport', 'cues', 'editor'],
    dock: null,
    defaults: timing,
  },
  editorAbove: {
    label: 'Editor above list',
    rows: ['preview', 'transport', 'editor', 'cues'],
    dock: null,
    defaults: timing,
  },
  review: {
    label: 'Review',
    rows: ['preview', 'transport', 'cues', 'editor'],
    dock: 'review',
    defaults: timing,
  },
  translation: {
    label: 'Translation',
    rows: ['preview', 'transport', 'cues', 'editor'],
    dock: 'translation',
    defaults: timing,
  },
  bigPicture: {
    label: 'Big picture',
    rows: ['preview', 'transport', 'cues', 'editor'],
    dock: null,
    defaults: { topPct: 40, editorPct: 16, wavePct: 35 },
  },
};
export function defaultLayout(): LayoutPreferences {
  return {
    active: 'timing',
    timing: { ...layoutPresets.timing.defaults },
    editorAbove: { ...layoutPresets.editorAbove.defaults },
    review: { ...layoutPresets.review.defaults },
    translation: { ...layoutPresets.translation.defaults },
    bigPicture: { ...layoutPresets.bigPicture.defaults },
  };
}
