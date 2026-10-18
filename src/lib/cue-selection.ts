// Stable IDs, file-order ranges, and one primary cue for the editor draft.
export type SelectionMode = 'replace' | 'toggle' | 'range' | 'addRange';
export interface CueSelection {
  ids: ReadonlySet<number>;
  primary: number | null;
  anchor: number | null;
}
export function selectCues(
  order: readonly { id: number }[],
  before: CueSelection,
  id: number | null,
  mode: SelectionMode,
): CueSelection {
  const index = order.findIndex((cue) => cue.id === id);
  const targetCue = order[index];
  if (!targetCue) return { ids: new Set(), primary: null, anchor: null };
  const target = targetCue.id;
  if (mode === 'replace')
    return { ids: new Set([target]), primary: target, anchor: target };
  const ids = new Set(mode === 'range' ? [] : before.ids);
  if (mode === 'toggle') {
    if (ids.has(target)) ids.delete(target);
    else ids.add(target);
    return {
      ids,
      primary: ids.has(target)
        ? target
        : (order.find((c) => ids.has(c.id))?.id ?? null),
      anchor: target,
    };
  }
  const anchorIndex = order.findIndex((c) => c.id === before.anchor);
  const anchor = anchorIndex < 0 ? index : anchorIndex;
  for (const cue of order.slice(
    Math.min(anchor, index),
    Math.max(anchor, index) + 1,
  ))
    ids.add(cue.id);
  return { ids, primary: target, anchor: order[anchor]?.id ?? target };
}
export function reconcileSelection(
  order: readonly { id: number }[],
  before: CueSelection,
  primary: number | null,
): CueSelection {
  const available = new Set(order.map((c) => c.id));
  const ids = new Set([...before.ids].filter((id) => available.has(id)));
  if (primary !== null && available.has(primary)) ids.add(primary);
  return {
    ids,
    primary,
    anchor:
      before.anchor !== null && available.has(before.anchor)
        ? before.anchor
        : primary,
  };
}

export function selectAllCues(
  order: readonly { id: number }[],
  primary: number | null,
): CueSelection {
  return { ids: new Set(order.map((c) => c.id)), primary, anchor: primary };
}
