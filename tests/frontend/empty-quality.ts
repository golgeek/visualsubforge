import type { QualityRequest, QualityView } from '../../src/lib/client';
import { qualityView } from './ipc-contract-samples';

/** Build the modeled worker reply from the fixture’s complete, unfiltered findings. */
export function fixtureIssueIndex(
  args: unknown,
  findings: QualityView['findings'],
) {
  const { request } = args as { request: QualityRequest };
  if (request.indexSince == null || request.indexSince === qualityView.reportId)
    return null;
  const violations = new Set<string>();
  const advisories = new Set<string>();
  for (const f of findings) {
    for (const id of [f.cueId, f.relatedCueId]) {
      if (id === null) continue;
      if (f.severity === 'violation') violations.add(id);
      else advisories.add(id);
    }
  }
  for (const id of violations) advisories.delete(id);
  return { violations: [...violations], advisories: [...advisories] };
}

/** Explicit empty report for App fixtures that do not test quality evaluation.
 * Quality/columns fixtures supply their own findings and paging behavior. */
export function emptyQuality(args: unknown): QualityView {
  const request = (args as { request: QualityRequest }).request;
  if (!request) throw new Error('Missing quality request in fixture');
  return {
    ...structuredClone(qualityView),
    id: 1,
    key: {
      ...qualityView.key,
      generation: request.generation,
      revision: request.revision,
      loaded: true,
    },
    phase: 'ready',
    issueIndex: fixtureIssueIndex(args, []),
    findings: [],
    metrics: [],
    selected: null,
    selectedIssues: null,
    draft: null,
    findingsCount: 0,
    timingFindingsCount: 0,
    affectedCues: 0,
    filteredCount: 0,
    filteredCues: 0,
    offset: 0,
    previousCue: null,
    nextCue: null,
    draftChecking: false,
    draftError: null,
    error: null,
  };
}
