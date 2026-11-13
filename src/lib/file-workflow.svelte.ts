import { tick } from 'svelte';
import { chooseFile, type ProjectReferenceIssue } from './client';
import type { Workspace } from './workspace.svelte';
import type { DocumentSession } from './document-session.svelte';
import type { TranslationSession } from './translation-session.svelte';
import type { ProjectWorkflow } from './project-workflow.svelte';
import type { WorkspaceView } from './workspace-view.svelte';
import type { WorkspaceDecision } from './workspace-decision.svelte';
import type { LegacyImportChoice } from './LegacyImportDialog.svelte';
import type { ProjectExitChoice } from './ProjectSaveFailureDialog.svelte';
interface Owners {
  workspace: Workspace;
  subtitles: DocumentSession;
  translation: TranslationSession;
  view: WorkspaceView;
  projects: ProjectWorkflow;
  legacyDecision: WorkspaceDecision<LegacyImportChoice | null>;
  subtitleDecision: WorkspaceDecision<'save' | 'discard' | 'cancel'>;
  projectExitDecision: WorkspaceDecision<ProjectExitChoice>;
  projectReferenceDecision: WorkspaceDecision<'cancel' | 'choose' | 'reload'>;
  alive(): boolean;
  showConfirmation(): void;
}
export function createFileWorkflow(owners: Owners) {
  const {
    workspace,
    subtitles,
    translation,
    view,
    legacyDecision,
    subtitleDecision,
    projectExitDecision,
    projectReferenceDecision,
  } = owners;
  async function closeLegacyImport(outputPath?: string, open = false) {
    if (outputPath) workspace.notice = `Project created at ${outputPath}`;
    await workspace.task(
      async () => {
        await legacyDecision.answer({ outputPath, open });
        const choice = await legacyDecision.wait();
        if (!owners.alive() || !choice) return;
        // Restoration completes before entering the existing protected open flow.
        if (choice.open && choice.outputPath)
          await owners.projects.openPath(choice.outputPath);
      },
      { clearError: false },
    );
    if (owners.alive()) {
      await tick();
      if (owners.alive() && view.legacyImportReturn?.isConnected)
        view.legacyImportReturn.focus();
    }
  }

  async function save(as: boolean) {
    const saved = await subtitles.save(as);
    if (saved) workspace.notice = 'Subtitles saved';
    return saved;
  }

  async function protect(
    saveContext?: () => Promise<boolean>,
  ): Promise<boolean> {
    if (!owners.alive()) return false;
    if (
      !subtitles.snapshot.dirty &&
      !subtitles.dirty &&
      !subtitles.snapshot.translation?.dirty
    )
      return true;
    const choice = await subtitleDecision.ask(() => owners.showConfirmation());
    if (!owners.alive() || choice === 'cancel') return false;
    if (choice === 'save') {
      return subtitles.snapshot.translation?.dirty
        ? (saveContext ?? (() => owners.projects.save(false)))()
        : save(false);
    }
    return true;
  }
  async function referenceOperation(
    operation: 'attach' | 'copy' | 'offset' | 'detach',
    offsetMs = 0,
  ) {
    if (!workspace.canAct) return;
    await workspace.task(async () => {
      if (operation === 'attach' || operation === 'copy') {
        const path = await chooseFile(
          'subtitles',
          subtitles.snapshot.translation?.reference?.path ??
            subtitles.snapshot.path,
        );
        if (!owners.alive() || !path) return;
        if (operation === 'copy' && !(await protect())) return;
        if (!owners.alive()) return;
        if (await translation.load(path, operation === 'copy')) {
          workspace.notice =
            operation === 'copy'
              ? 'Independent translation target created · Save As required'
              : 'Reference attached';
          ++view.qualityContext;
        }
      } else if (operation === 'offset') await translation.setOffset(offsetMs);
      else await translation.detach();
    });
  }
  $effect(() => {
    if (!view.translationOpen)
      translation.request(subtitles.snapshot, null, 0, null, false);
  });
  function resolveProjectReference(issue: ProjectReferenceIssue) {
    return projectReferenceDecision.ask(() => {
      view.projectReferenceIssue = issue;
    });
  }
  function projectSaveFailed(failure: unknown): Promise<ProjectExitChoice> {
    return projectExitDecision.ask(() => {
      view.projectExitError = String(failure);
    });
  }

  return {
    save,
    protect,
    referenceOperation,
    closeLegacyImport,
    resolveProjectReference,
    projectSaveFailed,
  };
}
