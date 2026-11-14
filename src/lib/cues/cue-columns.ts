import type { Cue, Preferences } from '../client';

export type CueColumns = Preferences['layout']['timing']['columns'];
export type CueColumn = CueColumns['items'][number];
export type ColumnId = CueColumn['id'];
export const columnLabels: Record<ColumnId, string> = {
  issue: 'Issues',
  number: '#',
  start: 'Start',
  end: 'End',
  duration: 'Duration',
  cps: 'CPS',
  rs: 'RS',
  longestLine: 'Longest line',
  lineCount: 'Line count',
  gap: 'Gap to next',
  style: 'Style',
  actor: 'Actor',
  text: 'Text',
};
export const metricColumns = new Set<ColumnId>([
  'duration',
  'cps',
  'rs',
  'longestLine',
  'lineCount',
  'gap',
]);
export function defaultColumns(): CueColumns {
  return {
    density: 'compact',
    items: [
      { id: 'issue', visible: true, width: 24 },
      { id: 'number', visible: true, width: 44 },
      { id: 'start', visible: true, width: 104 },
      { id: 'end', visible: true, width: 104 },
      { id: 'duration', visible: false, width: 80 },
      { id: 'cps', visible: false, width: 76 },
      { id: 'rs', visible: false, width: 64 },
      { id: 'longestLine', visible: false, width: 100 },
      { id: 'lineCount', visible: false, width: 80 },
      { id: 'gap', visible: false, width: 90 },
      { id: 'style', visible: false, width: 120 },
      { id: 'actor', visible: false, width: 120 },
      { id: 'text', visible: true, width: 240 },
    ],
  };
}
export const mandatoryColumn = (id: ColumnId) =>
  id === 'issue' || id === 'text';
export const availableColumn = (id: ColumnId, styled: boolean) =>
  styled || (id !== 'style' && id !== 'actor');
export const rowHeight = (density: CueColumns['density']) =>
  density === 'compact' ? 24 : 32;
export function visibleColumns(value: CueColumns, styled: boolean) {
  return value.items.filter((c) => c.visible && availableColumn(c.id, styled));
}
export function showColumn(
  value: CueColumns,
  id: ColumnId,
  visible: boolean,
): CueColumns {
  return {
    ...value,
    items: value.items.map((c) =>
      c.id === id ? { ...c, visible: mandatoryColumn(id) || visible } : c,
    ),
  };
}
export function resizeColumn(
  value: CueColumns,
  id: ColumnId,
  width: number,
): CueColumns {
  if (!Number.isFinite(width) || id === 'issue') return value;
  const minimum = id === 'text' ? 120 : 40;
  const maximum = id === 'text' ? 1600 : 600;
  // Whole CSS pixels at the persisted presentation boundary.
  const accepted = Math.round(Math.min(maximum, Math.max(minimum, width)));
  return {
    ...value,
    items: value.items.map((c) =>
      c.id === id ? { ...c, width: accepted } : c,
    ),
  };
}
export function moveColumn(
  value: CueColumns,
  id: ColumnId,
  target: ColumnId,
): CueColumns {
  if (id === 'issue' || id === target) return value;
  const from = value.items.findIndex((c) => c.id === id);
  const to = value.items.findIndex((c) => c.id === target);
  if (from < 1 || to < 1) return value;
  const items = [...value.items];
  const [column] = items.splice(from, 1);
  items.splice(to, 0, column!);
  return { ...value, items };
}
export function columnGrid(columns: readonly CueColumn[]) {
  return columns
    .map((c) =>
      c.id === 'text' ? `minmax(${c.width}px, 1fr)` : `${c.width}px`,
    )
    .join(' ');
}
export interface CueRow {
  cue: Cue;
  index: number;
}
/** Filtering preserves original file positions and stable cue identities. */
export function filterCueRows(
  cues: readonly Cue[],
  query: string,
  issuesOnly: boolean,
  issues: ReadonlyMap<string, unknown>,
): CueRow[] {
  const text = query.trim().toLowerCase();
  const number = /^#(\d+)$/.exec(text);
  return cues.flatMap((cue, index) => {
    const matches =
      !text ||
      (number
        ? index + 1 === Number(number[1])
        : cue.text.toLowerCase().includes(text));
    return matches && (!issuesOnly || issues.has(String(cue.id)))
      ? [{ cue, index }]
      : [];
  });
}
