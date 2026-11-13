import type { Workspace } from './workspace.svelte';
import type { DocumentSession } from './document-session.svelte';
import type { ProjectWorkflow } from './project-workflow.svelte';
import type { TimingGesture } from './timing-gesture.svelte';
import { chooseFile } from './client';
import type { WorkspaceDecision } from './workspace-decision.svelte';
import type { LegacyImportChoice } from './LegacyImportDialog.svelte';
import type { CueTableView } from './cue-table-view.svelte';

export type FileAction =
  | 'openProject'
  | 'saveProject'
  | 'saveProjectAs'
  | 'importLegacyProject'
  | 'openSubtitles'
  | 'closeSubtitles'
  | 'save'
  | 'saveAs'
  | 'quit';
interface Owners {
  workspace: Workspace;
  subtitles: DocumentSession;
  projects: ProjectWorkflow;
  gestures: TimingGesture;
  tableView: CueTableView;
  legacyDecision: WorkspaceDecision<LegacyImportChoice | null>;
  protect: () => Promise<boolean>;
  save: (as: boolean) => Promise<boolean>;
  resetScroll: () => void;
  legacyImport: { returnFocus: HTMLElement | null; open: boolean };
}

export async function runFileAction(action: FileAction, owners: Owners) {
  const {
    workspace,
    subtitles,
    projects,
    gestures,
    tableView,
    legacyDecision,
    protect,
    save,
    resetScroll,
    legacyImport,
  } = owners;
  gestures.cancelHeld();
  workspace.notice = '';
  await workspace.task(async () => {
    switch (action) {
      case 'openProject':
        await projects.open();
        break;
      case 'importLegacyProject':
        legacyImport.returnFocus =
          document.activeElement instanceof HTMLElement
            ? document.activeElement
            : null;
        await legacyDecision.open(() => {
          legacyImport.open = true;
        });
        break;
      case 'saveProject':
        await projects.save(false);
        break;
      case 'saveProjectAs':
        await projects.save(true);
        break;
      case 'openSubtitles': {
        const path = await chooseFile('subtitles', subtitles.snapshot.path);
        if (path && (await protect())) {
          await subtitles.command({ kind: 'open', path, discard: true });
          tableView.scrollTop = 0;
          resetScroll();
        }
        break;
      }
      case 'closeSubtitles':
        if (await protect())
          await subtitles.command({ kind: 'close', discard: true });
        break;
      case 'save':
        if (subtitles.snapshot.loaded) await save(false);
        break;
      case 'saveAs':
        if (subtitles.snapshot.loaded) await save(true);
        break;
      case 'quit':
        await projects.quit();
        break;
    }
  });
}
