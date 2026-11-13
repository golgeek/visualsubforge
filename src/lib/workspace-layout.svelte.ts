import { clamp } from './timeline';
import { paneLayout } from './pane-layout';
import type { WorkspaceView } from './workspace-view.svelte';
import type { DocumentSession } from './document-session.svelte';
export class WorkspaceLayout {
  constructor(
    private view: WorkspaceView,
    private subtitles: DocumentSession,
  ) {}
  workspaceHeight = $state(600);
  topPct = $state(32);
  editorPct = $state(25);
  wavePct = $state(65);
  editorBarHeight = $state(0);
  editorFooterHeight = $state(0);
  qualityReadoutHeight = $state(0);
  transportHeight = $state(0);
  cueOperationsHeight = $state(0);
  cueFollowingHeight = $state(0);
  resizeWaveform(value: number) {
    this.wavePct = clamp(value, 35, 78);
  }
  resizePreview(value: number) {
    this.topPct = clamp(value, 20, Math.min(55, 80 - this.editorPct));
  }
  resizeEditor(value: number) {
    this.editorPct = clamp(100 - value, 20, Math.min(40, 80 - this.topPct));
  }
  measureEditor(part: 'bar' | 'footer' | 'metrics', height: number) {
    if (part === 'bar') this.editorBarHeight = height;
    else if (part === 'footer') this.editorFooterHeight = height;
    else this.qualityReadoutHeight = height;
  }
  get panes() {
    return paneLayout({
      workspaceHeight: this.workspaceHeight,
      topPct: this.topPct,
      editorPct: this.editorPct,
      showQualityMetrics: this.view.showQualityMetrics,
      hasCue: !!this.subtitles.cue,
      editorBarHeight: this.editorBarHeight,
      editorFooterHeight: this.editorFooterHeight,
      qualityReadoutHeight: this.qualityReadoutHeight,
      transportHeight: this.transportHeight,
      cueOperationsHeight: this.cueOperationsHeight,
      cueFollowingHeight: this.cueFollowingHeight,
    });
  }
}
