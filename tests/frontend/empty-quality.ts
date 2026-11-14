import type { QualityRequest, QualityView } from '../../src/lib/client';
import { qualityView } from './ipc-contract-samples';

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
    findings: [],
    metrics: [],
    selected: null,
    selectedIssues: null,
    draft: null,
    findingsCount: 0,
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
