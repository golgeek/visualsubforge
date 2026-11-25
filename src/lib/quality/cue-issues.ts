import type { QualityView } from '../client';
export type IssueSeverity = QualityView['findings'][number]['severity'];
export function cueIssueMap(
  index: NonNullable<QualityView['issueIndex']>,
): ReadonlyMap<string, IssueSeverity> {
  const issues = new Map<string, IssueSeverity>();
  for (const id of index.advisories) issues.set(id, 'advisory');
  for (const id of index.violations) issues.set(id, 'violation');
  return issues;
}

/** Rust supplies a complete index even after missed revisions. Only changed
 * keys notify row and waveform consumers of the reactive display map. */
export function reconcileCueIssues(
  displayed: Map<string, IssueSeverity>,
  index: NonNullable<QualityView['issueIndex']>,
) {
  const next = cueIssueMap(index);
  for (const id of displayed.keys()) if (!next.has(id)) displayed.delete(id);
  for (const [id, severity] of next)
    if (displayed.get(id) !== severity) displayed.set(id, severity);
}
