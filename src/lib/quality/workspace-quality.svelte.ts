import type { DocumentSession } from '../document/document-session.svelte';
import type { MediaSession } from '../media/media-session.svelte';
import type { Workspace } from '../workspace/workspace.svelte';
import type { WorkspacePreferences } from '../settings/workspace-preferences.svelte';
import type { WorkspaceScenes } from '../timeline/workspace-scenes.svelte';
import type { QualitySession } from './quality-session.svelte';
import type { CueTableView } from '../cues/cue-table-view.svelte';
import { parseTime } from '../timeline/timeline';
export class WorkspaceQuality {
  draft = $derived.by(() => {
    const cue = this.subtitles.cue;
    if (!this.subtitles.dirty || !cue) return { value: null, error: '' };
    try {
      return {
        value: {
          cueId: cue.id,
          startMs: parseTime(this.subtitles.start),
          endMs: parseTime(this.subtitles.end),
          text: this.subtitles.text,
        },
        error: '',
      };
    } catch (failure) {
      return { value: null, error: String(failure) };
    }
  });
  constructor(
    private workspace: Workspace,
    private subtitles: DocumentSession,
    private media: MediaSession,
    private quality: QualitySession,
    private tableView: CueTableView,
    private preferences: WorkspacePreferences,
    private scenes: WorkspaceScenes,
  ) {
    $effect(() => {
      this.quality.configure({
        session: this.media.status.session,
        enabled: this.workspace.ready && this.subtitles.snapshot.loaded,
        generation: this.subtitles.snapshot.generation,
        revision: this.subtitles.snapshot.revision,
        context: `${this.media.status.session}/${this.preferences.qualityContext}/${this.scenes.sceneState.qualityKey}`,
        selected: this.subtitles.selected,
        indexIssues: true,
        metricsOffset: this.tableView.metricOffsets[0] ?? 0,
        metricsLimit:
          this.tableView.needsMetrics || this.preferences.showQualityMetrics
            ? 200
            : 0,
        metricOffsets:
          this.tableView.needsMetrics || this.preferences.showQualityMetrics
            ? this.tableView.metricOffsets
            : [],
        draft: this.preferences.showQualityMetrics ? this.draft.value : null,
      });
    });
  }
}
