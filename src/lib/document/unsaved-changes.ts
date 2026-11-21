import type { Workspace } from '../workspace/workspace.svelte';
import type { DocumentSession } from './document-session.svelte';
import type { WorkspaceDecision } from '../dialogs/workspace-decision.svelte';
export type UnsavedDecision =
  'continue' | 'discard' | 'cancel' | { save: () => Promise<boolean> };

/** Saving and the unsaved-changes guard; callers supply the relevant project save. */
export class UnsavedChanges {
  private alive = true;
  constructor(
    private workspace: Workspace,
    private subtitles: DocumentSession,
    private decision: WorkspaceDecision<
      'save' | 'discard' | 'cancel',
      'review'
    >,
    private show: () => void,
  ) {}
  save = async (as: boolean) => {
    const saved = await this.subtitles.save(as);
    if (saved) this.workspace.notice = 'Subtitles saved';
    return saved;
  };
  decide = async (
    saveContext: () => Promise<boolean>,
    review = false,
  ): Promise<UnsavedDecision> => {
    if (!this.alive) return 'cancel';
    if (
      !review &&
      !this.subtitles.snapshot.dirty &&
      !this.subtitles.dirty &&
      !this.subtitles.snapshot.translation?.dirty &&
      !this.subtitles.snapshot.review?.dirty
    )
      return 'continue';
    const choice = await this.decision.ask(() => {
      if (review) this.decision.payload = 'review';
      this.show();
    });
    if (!this.alive) return 'cancel';
    if (choice !== 'save') return choice;
    const project =
      review ||
      this.subtitles.snapshot.translation?.dirty ||
      this.subtitles.snapshot.review?.dirty;
    return { save: project ? saveContext : () => this.save(false) };
  };
  protect = async (saveContext: () => Promise<boolean>): Promise<boolean> => {
    const decision = await this.decide(saveContext);
    return typeof decision === 'object'
      ? decision.save()
      : decision !== 'cancel';
  };
  dispose() {
    this.alive = false;
  }
}
