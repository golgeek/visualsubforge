import type { Workspace } from './workspace.svelte';
import type { DocumentSession } from '../document/document-session.svelte';
import type { ProjectWorkflow } from '../project/project-workflow.svelte';
import type { TimingGesture } from '../timing/timing-gesture.svelte';
import { chooseFile } from '../client';
import type { LegacyImportDecision } from '../project/legacy-import-decision.svelte';
import type { CueTableView } from '../cues/cue-table-view.svelte';

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
  legacyDecision: LegacyImportDecision;
  protect: () => Promise<boolean>;
  save: (as: boolean) => Promise<boolean>;
  resetScroll: () => void;
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
  } = owners;
  gestures.cancelHeld();
  workspace.notice = '';
  await workspace.task(async () => {
    switch (action) {
      case 'openProject':
        await projects.open();
        break;
      case 'importLegacyProject':
        await legacyDecision.openImport();
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
