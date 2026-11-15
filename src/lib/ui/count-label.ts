/** A count and its noun, shared by compact summaries and detailed panels. */
export function countLabel(
  count: number | string,
  singular: string,
  plural = `${singular}s`,
): string {
  return `${count} ${count === 1 ? singular : plural}`;
}
