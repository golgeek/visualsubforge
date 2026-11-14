import type { Workspace } from '../workspace/workspace.svelte';
import type { DocumentSession } from './document-session.svelte';
import type { WorkspaceDecision } from '../dialogs/workspace-decision.svelte';
/** Saving and the unsaved-changes guard; callers supply the relevant project save. */
export class UnsavedChanges {
  private alive = true;
  constructor(
    private workspace: Workspace,
    private subtitles: DocumentSession,
    private decision: WorkspaceDecision<'save' | 'discard' | 'cancel'>,
    private show: () => void,
  ) {}
  save = async (as: boolean) => {
    const saved = await this.subtitles.save(as);
    if (saved) this.workspace.notice = 'Subtitles saved';
    return saved;
  };
  protect = async (saveContext: () => Promise<boolean>): Promise<boolean> => {
    if (!this.alive) return false;
    if (
      !this.subtitles.snapshot.dirty &&
      !this.subtitles.dirty &&
      !this.subtitles.snapshot.translation?.dirty
    )
      return true;
    const choice = await this.decision.ask(this.show);
    if (!this.alive || choice === 'cancel') return false;
    if (choice === 'save')
      return this.subtitles.snapshot.translation?.dirty
        ? saveContext()
        : this.save(false);
    return true;
  };
  dispose() {
    this.alive = false;
  }
}
