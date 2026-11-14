import type { LayoutPreset } from './layout-presets';
import type { Preferences } from '../client';

export type Arrangement = NonNullable<Preferences['layout']['custom']>;
export type Panel = Arrangement['rows'][number]['panels'][number];
export const panelNames: Record<Panel, string> = {
  waveform: 'Waveform',
  video: 'Video',
  transport: 'Transport',
  cues: 'Subtitle list',
  editor: 'Cue editor',
  review: 'Review / Statistics',
  translation: 'Translation',
};
export const panels = Object.keys(panelNames) as Panel[];
export interface Space {
  width: number;
  height: number;
  editor: number;
  transport: number;
}
export type Move =
  | {
      panel: Panel;
      target: Panel;
      kind: 'swap' | 'before' | 'after' | 'beside';
    }
  | { panel: Panel; kind: 'hide' };
export function presetArrangement(
  preset: LayoutPreset,
  wave: number,
  review: boolean,
  translation: boolean,
): Arrangement {
  const editor = { panels: ['editor' as const], weight: 16, split: 50 };
  const cues = {
    panels: [
      'cues' as Panel,
      ...(review
        ? ['review' as const]
        : translation
          ? ['translation' as const]
          : []),
    ],
    weight: 60,
    split: 58,
  };
  return {
    version: 1,
    rows: [
      {
        panels: ['waveform', 'video'],
        weight: preset === 'bigPicture' ? 40 : 13,
        split: Math.round(wave),
      },
      { panels: ['transport'], weight: 1, split: 50 },
      ...(preset === 'editorAbove' ? [editor, cues] : [cues, editor]),
    ],
  };
}
export function minimum(panel: Panel, space: Space) {
  switch (panel) {
    case 'waveform':
      return { width: 300, height: 100 };
    case 'video':
      return { width: 150, height: 100 };
    case 'transport':
      return { width: 900, height: Math.max(38, space.transport) };
    case 'editor':
      return { width: 600, height: Math.max(132, space.editor) };
    case 'cues':
      return { width: 350, height: 160 };
    case 'review':
    case 'translation':
      return { width: 320, height: 160 };
  }
}
export function arrangementGeometry(value: Arrangement, space: Space) {
  const rows = value.rows.map((row) => {
    // Rust validates one or two unique panels per row at the boundary.
    const needs = row.panels.map((panel) => minimum(panel, space));
    const ratio = row.split / 100;
    const width =
      needs.length === 1
        ? needs[0]!.width
        : Math.max(
            (needs[0]!.width + 3) / ratio,
            (needs[1]!.width + 3) / (1 - ratio),
          );
    return { width, height: Math.max(...needs.map((n) => n.height)) };
  });
  const width = Math.max(...rows.map((row) => row.width));
  const height =
    rows.reduce((sum, row) => sum + row.height, 0) + (rows.length - 1) * 6;
  const extra = Math.max(0, space.height - height);
  const weights = value.rows.map((row) =>
    row.panels.length === 1 && row.panels[0] === 'transport' ? 0 : row.weight,
  );
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  return {
    width,
    height,
    rows: rows.map(
      (row, index) =>
        row.height + (total ? (extra * weights[index]!) / total : 0),
    ),
  };
}
export function refusal(value: Arrangement, space: Space): string | null {
  const geometry = arrangementGeometry(value, space);
  if (geometry.width > space.width + 1)
    return `This arrangement needs ${Math.ceil(geometry.width)} px of width; ${Math.floor(space.width)} px is available.`;
  if (geometry.height > space.height + 1)
    return `This arrangement needs ${Math.ceil(geometry.height)} px of height; ${Math.floor(space.height)} px is available. Hide a panel or place two panels in one row.`;
  return null;
}
export function movePanel(value: Arrangement, move: Move): Arrangement {
  const next = structuredClone(value);
  const source = next.rows.find((row) => row.panels.includes(move.panel));
  if (move.kind === 'hide') {
    if (next.rows.flatMap((row) => row.panels).length === 1)
      throw new Error('Keep at least one panel visible.');
    if (source)
      source.panels = source.panels.filter((panel) => panel !== move.panel);
  } else {
    if (move.panel === move.target) return next;
    let target = next.rows.find((row) => row.panels.includes(move.target));
    if (!target) throw new Error('The destination panel is no longer visible.');
    if (move.kind === 'swap') {
      const index = target.panels.indexOf(move.target);
      if (source)
        source.panels[source.panels.indexOf(move.panel)] = move.target;
      target.panels[index] = move.panel;
    } else {
      if (source)
        source.panels = source.panels.filter((panel) => panel !== move.panel);
      next.rows = next.rows.filter((row) => row.panels.length);
      target = next.rows.find((row) => row.panels.includes(move.target))!;
      if (move.kind === 'beside') {
        if (target.panels.length === 2)
          throw new Error(
            'A row has at most two panels. Swap or insert a row instead.',
          );
        target.panels.push(move.panel);
        target.split = 50;
      } else {
        const index =
          next.rows.indexOf(target) + (move.kind === 'after' ? 1 : 0);
        next.rows.splice(index, 0, {
          panels: [move.panel],
          weight: 20,
          split: 50,
        });
      }
    }
  }
  next.rows = next.rows.filter((row) => row.panels.length);
  return next;
}
