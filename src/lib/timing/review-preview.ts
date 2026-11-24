import type {
  DocumentSnapshot,
  TimingFixView,
  BatchTimingView,
} from '../client';
export type TimingView = TimingFixView | BatchTimingView;
export interface TimingGhost {
  id: number;
  startMs: number;
  endMs: number;
  oldStartMs: number;
  oldEndMs: number;
  updating?: boolean;
}
export interface TimingPreview {
  document: DocumentSnapshot;
  ghosts: TimingGhost[];
  retained?: { session: number; context: string };
}
/** Projection of Rust-calculated values, never an accepted document edit. */
export function timingGhosts(view: TimingView): TimingGhost[] {
  if (view.phase !== 'reviewing' || view.selectionError) return [];
  if ('manualCount' in view) {
    const ghosts = new Map<number, TimingGhost>();
    for (const row of view.rows)
      if (row.selected)
        for (const g of row.ghosts) {
          if (
            g.after.startMs === null ||
            g.after.endMs === null ||
            g.before.startMs === null ||
            g.before.endMs === null
          )
            continue;
          const id = Number(g.cueId);
          ghosts.set(id, {
            id,
            startMs: g.after.startMs,
            endMs: g.after.endMs,
            oldStartMs: g.before.startMs,
            oldEndMs: g.before.endMs,
          });
        }
    return [...ghosts.values()].slice(0, 200);
  }
  return view.rows
    .filter((row) => row.selected)
    .map((row) => ({
      id: Number(row.cueId),
      startMs: row.newStartMs,
      endMs: row.newEndMs,
      oldStartMs: row.oldStartMs,
      oldEndMs: row.oldEndMs,
    }));
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
