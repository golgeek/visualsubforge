import type { StatisticsSelection } from './client';
export function statisticsScope(
  scope: 'document' | 'range',
  first: string,
  last: string,
  excluded: string,
): { selection: StatisticsSelection | null; error: string } {
  const integer = (s: string) =>
    /^\d+$/.test(s) &&
    Number.isSafeInteger(Number(s)) &&
    Number(s) > 0 &&
    Number(s) <= 100000;
  if (scope === 'range' && (!integer(first) || !integer(last)))
    return {
      selection: null,
      error:
        'Enter whole cue numbers from 1 to 100000 for both range endpoints.',
    };
  const numbers = excluded.trim()
    ? excluded.split(',').map((s) => s.trim())
    : [];
  if (numbers.length > 100000 || numbers.some((s) => !integer(s)))
    return {
      selection: null,
      error: 'Enter excluded cue numbers separated by commas.',
    };
  return {
    selection: {
      range:
        scope === 'range' ? { first: Number(first), last: Number(last) } : null,
      excluded: numbers.map(Number),
    },
    error: '',
  };
}
