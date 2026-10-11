/** Transient playback selection policy; never edits a document or seeks media. */
export interface TimedCue {
  id: number;
  startMs: number;
  endMs: number;
}
export type CueFollowing = 'off' | 'following' | 'suspended';
export type FollowAction = 'enable' | 'disable' | 'suspend' | 'resume';

export function changeFollowing(
  current: CueFollowing,
  action: FollowAction,
  protectedEdit: boolean,
): CueFollowing {
  if (action === 'disable') return 'off';
  if (action === 'suspend') return current === 'off' ? 'off' : 'suspended';
  if (action === 'resume' && current === 'off') return 'off';
  return protectedEdit ? 'suspended' : 'following';
}

/** Compare fractional milliseconds without rounding into a neighboring cue.
 * Document order, rather than chronological sorting, breaks overlap ties.
 * Empty/invalid intervals cannot become active (the Rust domain rejects them).
 */
export function activeCueIds(
  cues: readonly TimedCue[],
  positionSeconds: number | null,
): number[] {
  if (positionSeconds === null || !Number.isFinite(positionSeconds)) return [];
  return cues
    .filter(
      (cue) =>
        cue.startMs < cue.endMs &&
        cue.startMs / 1000 <= positionSeconds &&
        positionSeconds < cue.endMs / 1000,
    )
    .map((cue) => cue.id);
}

export function followedSelection(
  activeIds: readonly number[],
  selected: number | null,
  mode: CueFollowing,
  protectedEdit: boolean,
): number | null {
  if (mode !== 'following' || protectedEdit || activeIds.length === 0)
    return selected;
  return selected !== null && activeIds.includes(selected)
    ? selected
    : (activeIds[0] ?? null);
}

/** Reveal a fixed-height virtual row within its list only; never move focus. */
export function revealCueRow(
  index: number,
  scrollTop: number,
  viewportHeight: number,
  rowHeight = 32,
): number {
  const top = index * rowHeight;
  if (top < scrollTop) return top;
  if (top + rowHeight > scrollTop + viewportHeight)
    return Math.max(0, top + rowHeight - viewportHeight);
  return scrollTop;
}
