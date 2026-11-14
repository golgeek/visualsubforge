import type { Cue, StatisticsSelection } from '../client';
/** Translate accepted stable IDs to the existing Rust statistics position contract.
 * Whole-document evaluation still happens in Rust before this display scope. */
export function statisticsSelection(
  cues: readonly Pick<Cue, 'id'>[],
  ids: readonly number[] | null,
): { selection: StatisticsSelection | null; error: string } {
  if (ids === null)
    return { selection: { range: null, excluded: [] }, error: '' };
  const known = new Set(cues.map((cue) => cue.id));
  if (ids.some((id) => !known.has(id)))
    return {
      selection: null,
      error: 'List scope changed; select current cues again.',
    };
  const selected = new Set(ids);
  return {
    selection: {
      range: null,
      excluded: cues.flatMap((cue, index) =>
        selected.has(cue.id) ? [] : [index + 1],
      ),
    },
    error: '',
  };
}
