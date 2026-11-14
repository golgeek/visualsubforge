import type {
  DocumentSnapshot,
  GapView,
  SceneRepairView,
  DurationRepairView,
  BatchTimingView,
} from '../client';
export type TimingView =
  GapView | SceneRepairView | DurationRepairView | BatchTimingView;
export interface TimingGhost {
  id: number;
  startMs: number;
  endMs: number;
  oldStartMs: number;
  oldEndMs: number;
}
export interface TimingPreview {
  document: DocumentSnapshot;
  ghosts: TimingGhost[];
}
/** Projection of Rust-calculated values, never an accepted document edit. */
export function timingGhosts(view: TimingView): TimingGhost[] {
  if (view.phase !== 'reviewing' || view.selectionError) return [];
  return view.rows.flatMap((row) => {
    if (!row.selected) return [];
    const oldStartMs =
      'oldStartMs' in row ? row.oldStartMs : row.before.data.startMs;
    const oldEndMs = 'oldEndMs' in row ? row.oldEndMs : row.before.data.endMs;
    if (oldStartMs === null || oldEndMs === null) return [];
    return [
      {
        id: Number(row.cueId),
        oldStartMs,
        oldEndMs,
        startMs: ('newStartMs' in row ? row.newStartMs : null) ?? oldStartMs,
        endMs: row.newEndMs ?? oldEndMs,
      },
    ];
  });
}
export function timingDelta(ghost: TimingGhost): string {
  const signed = (n: number) => `${n > 0 ? '+' : ''}${n} ms`;
  const start = ghost.startMs - ghost.oldStartMs,
    end = ghost.endMs - ghost.oldEndMs;
  return start === end
    ? signed(start)
    : [start ? `start ${signed(start)}` : '', end ? `end ${signed(end)}` : '']
        .filter(Boolean)
        .join(' · ');
}
