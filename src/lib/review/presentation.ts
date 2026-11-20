import type { ReviewChange, ReviewReply } from '../client';
export type Side = 'original' | 'revised';
export type Summary = NonNullable<ReviewReply['presentation']>;
export type ReviewRow = Summary['rows'][number];
export type TableFilter = 'all' | 'changed' | 'threads';

export function diffParts(
  words: ReviewChange['words'],
  side: Side,
  singleLine = false,
) {
  return words
    .filter((w) => w.kind !== (side === 'original' ? 'added' : 'removed'))
    .flatMap((w) =>
      w.text
        .split(/(\n)/u)
        .filter(Boolean)
        .map((text) => ({
          kind: w.kind,
          text:
            text === '\n'
              ? singleLine
                ? '↵'
                : w.kind === 'equal'
                  ? '\n'
                  : '↵\n'
              : text,
        })),
    );
}

/** Tokens span the joined group. Slice them at the actual cue boundaries. */
export function cueWords(change: ReviewChange, side: Side, index: number) {
  const cues = change[side];
  if (cues.length <= 1) return change.words;
  const start = cues.slice(0, index).reduce((n, c) => n + c.text.length + 1, 0);
  const end = start + (cues[index]?.text.length ?? 0);
  let position = 0;
  return change.words.flatMap((word) => {
    if (word.kind === (side === 'original' ? 'added' : 'removed')) return [];
    const from = position;
    position += word.text.length;
    return position > start && from < end
      ? [
          {
            kind: word.kind,
            text: word.text.slice(Math.max(0, start - from), end - from),
          },
        ]
      : [];
  });
}
export function reviewRows(rows: readonly ReviewRow[]) {
  const changed = new Set<number>(),
    threaded = new Set<number>();
  const removals = new Map<number, ReviewRow[]>();
  const byCue = new Map<number, ReviewRow>();
  const bySource = new Map<string, ReviewRow>();
  for (const row of rows) {
    bySource.set(row.key + '/' + row.sourceIndex, row);
    for (const id of row.revised) {
      byCue.set(id, row);
      if (row.kind !== 'unchanged') changed.add(id);
      if (row.hasThreads) threaded.add(id);
    }
    if (row.nextCue != null) {
      changed.add(row.nextCue);
      if (row.hasThreads) threaded.add(row.nextCue);
      const group = removals.get(row.nextCue) ?? [];
      group.push(row);
      removals.set(row.nextCue, group);
    }
  }
  return { changed, threaded, removals, byCue, bySource };
}
export const checkKey = (row: ReviewRow) => row.key + '/' + row.occurrence;
export function retainedChecks(
  rows: readonly ReviewRow[],
  checked: ReadonlySet<string>,
) {
  return new Set(
    rows
      .filter((r) => r.kind !== 'unchanged')
      .map(checkKey)
      .filter((k) => checked.has(k)),
  );
}
export function exportProgress(
  rows: readonly ReviewRow[],
  checked: ReadonlySet<string>,
) {
  const changes = rows.filter((r) => r.kind !== 'unchanged');
  const unchecked = changes.filter((r) => !checked.has(checkKey(r)));
  return {
    total: changes.length,
    checked: changes.length - unchecked.length,
    unchecked,
  };
}
export function kindLabel(kind: ReviewChange['kind']) {
  return kind === 'text_and_timing'
    ? 'Text and timing'
    : kind[0]!.toUpperCase() + kind.slice(1);
}
export function signedDelta(ms: number) {
  return (ms >= 0 ? '+' : '') + (ms / 1000).toFixed(3) + ' s';
}
