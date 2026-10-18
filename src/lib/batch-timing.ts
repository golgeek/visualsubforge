import type { BatchTimingRequest } from './client';

// A derived input can be recomputed after controls stop observing it. Compare
// the actual request, not the identity of a transient derived object.
export function sameBatchRequest(
  a: BatchTimingRequest | null,
  b: BatchTimingRequest | null,
): boolean {
  if (!a || !b) return a === b;
  const x = a.change,
    y = b.change;
  const change =
    x.kind === 'align'
      ? y.kind === 'align' && x.playheadMs === y.playheadMs
      : y.kind === 'shift' &&
        x.earlier === y.earlier &&
        x.amountMs === y.amountMs;
  if (!change) return false;
  const p = a.scope,
    q = b.scope;
  switch (p.kind) {
    case 'all':
      return q.kind === 'all';
    case 'following':
      return q.kind === 'following' && p.cueId === q.cueId;
    case 'atOrAfter':
      return q.kind === 'atOrAfter' && p.positionMs === q.positionMs;
    case 'selected':
      return (
        q.kind === 'selected' &&
        p.cueIds.length === q.cueIds.length &&
        p.cueIds.every((id, i) => id === q.cueIds[i])
      );
  }
}
