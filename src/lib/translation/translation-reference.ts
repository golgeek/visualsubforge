import { chooseFile } from '../client';
import type { Workspace } from '../workspace/workspace.svelte';
import type { DocumentSession } from '../document/document-session.svelte';
import type { TranslationSession } from './translation-session.svelte';
import type { UnsavedChanges } from '../document/unsaved-changes';
import type { WorkspacePreferences } from '../settings/workspace-preferences.svelte';
export class TranslationReference {
  private alive = true;
  constructor(
    private workspace: Workspace,
    private subtitles: DocumentSession,
    private translation: TranslationSession,
    private unsaved: UnsavedChanges,
    private preferences: WorkspacePreferences,
    private saveProject: () => Promise<boolean>,
  ) {}
  run = async (
    operation: 'attach' | 'copy' | 'offset' | 'detach',
    offsetMs = 0,
  ) => {
    if (!this.workspace.canAct) return;
    await this.workspace.task(async () => {
      if (operation === 'attach' || operation === 'copy') {
        const path = await chooseFile(
          'subtitles',
          this.subtitles.snapshot.translation?.reference?.path ??
            this.subtitles.snapshot.path,
        );
        if (!this.alive || !path) return;
        if (
          operation === 'copy' &&
          !(await this.unsaved.protect(this.saveProject))
        )
          return;
        if (!this.alive) return;
        if (await this.translation.load(path, operation === 'copy')) {
          this.workspace.notice =
            operation === 'copy'
              ? 'Independent translation target created · Save As required'
              : 'Reference attached';
          this.preferences.invalidateQuality();
        }
      } else if (operation === 'offset')
        await this.translation.setOffset(offsetMs);
      else await this.translation.detach();
    });
  };
  dispose() {
    this.alive = false;
  }
}
