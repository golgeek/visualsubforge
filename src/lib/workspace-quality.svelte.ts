import type { DocumentSession } from './document-session.svelte';
import type { MediaSession } from './media-session.svelte';
import type { Workspace } from './workspace.svelte';
import type { WorkspaceView } from './workspace-view.svelte';
import type { QualitySession } from './quality-session.svelte';
import type { CueTableView } from './cue-table-view.svelte';
import { parseTime } from './timeline';
export function createWorkspaceQuality(
  workspace: Workspace,
  subtitles: DocumentSession,
  media: MediaSession,
  quality: QualitySession,
  tableView: CueTableView,
  view: WorkspaceView,
) {
  const doc = $derived(subtitles.snapshot);
  const selected = $derived(subtitles.selected);
  const draftDirty = $derived(subtitles.dirty);
  const status = $derived(media.status);
  const cue = $derived(subtitles.cue);
  const draftMetricInput = $derived.by(() => {
    if (!draftDirty || !cue) return { value: null, error: '' };
    try {
      return {
        value: {
          cueId: cue.id,
          startMs: parseTime(subtitles.start),
          endMs: parseTime(subtitles.end),
          text: subtitles.text,
        },
        error: '',
      };
    } catch (failure) {
      return { value: null, error: String(failure) };
    }
  });
  $effect(() => {
    quality.configure({
      session: status.session,
      enabled:
        workspace.ready &&
        doc.loaded &&
        (view.problemsOpen || view.showQualityMetrics),
      generation: doc.generation,
      revision: doc.revision,
      context: `${status.session}/${view.qualityContext}/${view.sceneState.qualityKey}`,
      selected,
      metricsOffset: tableView.firstRow,
      metricsLimit: view.showQualityMetrics ? tableView.visibleCues.length : 0,
      draft: view.showQualityMetrics ? draftMetricInput.value : null,
    });
  });

  return {
    get draft() {
      return draftMetricInput;
    },
  };
}
