import type { QualityView } from '../client';
export type IssueSeverity = QualityView['findings'][number]['severity'];
/** Index the Rust findings, including both cues in a gap/overlap pair. No rules are evaluated here. */
export function addIssuePage(
  current: ReadonlyMap<string, IssueSeverity>,
  findings: QualityView['findings'],
): ReadonlyMap<string, IssueSeverity> {
  const next = new Map(current);
  for (const finding of findings) {
    for (const id of [finding.cueId, finding.relatedCueId]) {
      if (id && next.get(id) !== 'violation') next.set(id, finding.severity);
    }
  }
  return next;
}
