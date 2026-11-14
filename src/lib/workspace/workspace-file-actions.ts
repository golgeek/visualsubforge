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
    legacyDecision,
    protect,
    save,
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
        if (path) await openSubtitlePath(owners, path);
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

/** Shared picker/drop/recent continuation; the caller owns the workspace task. */
export async function openSubtitlePath(owners: Owners, path: string) {
  const { workspace, subtitles, protect, tableView, resetScroll } = owners;
  if (workspace.disposed || !(await protect()) || workspace.disposed)
    return false;
  if (
    !(await subtitles.command({ kind: 'open', path, discard: true })) ||
    workspace.disposed
  )
    return false;
  tableView.scrollTop = 0;
  resetScroll();
  return true;
}
