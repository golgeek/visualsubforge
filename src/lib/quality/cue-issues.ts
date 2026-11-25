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
