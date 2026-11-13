import type { Workspace } from './workspace.svelte';
import type { CueNavigation } from './cue-navigation.svelte';
import type { WorkspaceDialogsState } from './workspace-dialogs.svelte';
import type { WorkspaceModal } from './workspace-modal.svelte';
import type { ProjectWorkflow } from './project-workflow.svelte';
import type { TranslationSession } from './translation-session.svelte';
import type { DocumentSession } from './document-session.svelte';
import type { QualitySession } from './quality-session.svelte';
import type { MediaSession } from './media-session.svelte';
import type { CueTableView } from './cue-table-view.svelte';
import type { TimingGesture } from './timing-gesture.svelte';
import type { TimelineView } from './timeline-view.svelte';
import type { Recovery } from './recovery.svelte';
import type { CueMenu } from './cue-menu.svelte';
import type { WorkspaceView } from './workspace-view.svelte';
import { defaultPreferences, getPreferences, workspaceActions } from './client';
interface Owners {
  workspace: Workspace;
  navigation: CueNavigation;
  dialogs: WorkspaceDialogsState;
  workspaceModal: WorkspaceModal;
  projects: ProjectWorkflow;
  translation: TranslationSession;
  subtitles: DocumentSession;
  quality: QualitySession;
  media: MediaSession;
  tableView: CueTableView;
  gestures: TimingGesture;
  timeline: TimelineView;
  recovery: Recovery;
  cueMenu: CueMenu;
  view: WorkspaceView;
  legacyDecision: { dispose(): void };
  subtitleDecision: { dispose(): void };
  projectExitDecision: { dispose(): void };
  projectReferenceDecision: { dispose(): void };
}
export class WorkspaceLifetime {
  alive = true;
  mount(
    owners: Owners,
    dom: { video(): HTMLDivElement; list(): HTMLDivElement | undefined },
  ) {
    const {
      workspace,
      navigation,
      dialogs,
      workspaceModal,
      projects,
      translation,
      subtitles,
      quality,
      media,
      tableView,
      gestures,
      timeline,
      recovery,
      cueMenu,
      view,
      legacyDecision,
      subtitleDecision,
      projectExitDecision,
      projectReferenceDecision,
    } = owners;

    this.alive = true;
    let unlisten = () => {};
    void Promise.all([
      subtitles.command({ kind: 'status' }),
      getPreferences().catch((e) => {
        if (this.alive) workspace.error = String(e);
        return defaultPreferences;
      }),
    ])
      .then(([, saved]) => {
        if (this.alive) {
          view.preferences = { ...saved, timing: { ...saved.timing } };
          view.timingSettings = { ...saved.timing };
          view.showQualityMetrics = saved.showQualityMetrics;
        }
      })
      .catch((e) => {
        if (this.alive) workspace.error = String(e);
      })
      .finally(() => {
        if (this.alive) workspace.ready = true;
      });
    void workspaceActions((action) => void workspace.run(action))
      .then((stop) => {
        if (this.alive) unlisten = stop;
        else stop();
      })
      .catch((e) => (workspace.error = String(e)));
    let layoutPending = false;
    let lastLayout = '';
    const layout = async () => {
      if (!this.alive || layoutPending || !media.status.available) return;
      const rect = dom.video().getBoundingClientRect();
      const bounds = {
        kind: 'layout' as const,
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        visible:
          media.status.loaded &&
          rect.bottom > 0 &&
          rect.right > 0 &&
          rect.top < innerHeight &&
          rect.left < innerWidth,
      };
      const key = JSON.stringify([
        bounds,
        innerWidth,
        innerHeight,
        devicePixelRatio,
      ]);
      if (key === lastLayout) return;
      layoutPending = true;
      try {
        await media.layout(bounds);
        if (this.alive) lastLayout = key;
      } catch (e) {
        if (this.alive) workspace.error = String(e);
      } finally {
        layoutPending = false;
      }
    };
    const observer = new ResizeObserver(() => void layout());
    observer.observe(dom.video());
    const listObserver = new ResizeObserver(([entry]) => {
      if (entry) tableView.height = entry.contentRect.height;
    });
    const list = dom.list();
    if (list) listObserver.observe(list);
    window.addEventListener('resize', layout);
    window.addEventListener('scroll', layout, true);
    timeline.startPolling();
    const layoutTimer = setInterval(() => void layout(), 100);
    recovery.startPolling();
    media.startPolling();
    return () => {
      this.alive = false;
      workspace.dispose();
      navigation.dispose();
      dialogs.dispose();
      workspaceModal.dispose();
      projects.dispose();
      translation.dispose();
      subtitles.dispose();
      quality.dispose();
      media.dispose();
      legacyDecision.dispose();
      subtitleDecision.dispose();
      projectExitDecision.dispose();
      projectReferenceDecision.dispose();
      tableView.dispose();
      gestures.dispose();
      timeline.dispose();
      unlisten();
      observer.disconnect();
      listObserver.disconnect();
      clearInterval(layoutTimer);
      recovery.dispose();
      cueMenu.dispose();
      window.removeEventListener('resize', layout);
      window.removeEventListener('scroll', layout, true);
    };
  }
}
