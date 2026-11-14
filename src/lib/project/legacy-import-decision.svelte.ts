import { tick } from 'svelte';
import {
  WorkspaceDecision,
  decisionEffects,
} from '../dialogs/workspace-decision.svelte';
import type { Workspace } from '../workspace/workspace.svelte';
import type { MediaSession } from '../media/media-session.svelte';
import type { ProjectWorkflow } from './project-workflow.svelte';
import type { LegacyImportChoice } from './LegacyImportDialog.svelte';
export class LegacyImportDecision extends WorkspaceDecision<
  LegacyImportChoice | null,
  { path: string | null }
> {
  private returnFocus: HTMLElement | null = null;
  constructor(
    private controls: Workspace,
    media: MediaSession,
    occlusion: () => boolean,
  ) {
    super(null, controls, decisionEffects(media, occlusion));
  }
  openImport(path: string | null = null) {
    this.returnFocus =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    return this.show({ path });
  }
  async finish(projects: ProjectWorkflow, outputPath?: string, open = false) {
    if (outputPath) this.controls.notice = `Project created at ${outputPath}`;
    await this.controls.task(
      async () => {
        await this.answer({ outputPath, open });
        const choice = await this.wait();
        if (this.disposed || !choice) return;
        // Video restoration finishes before entering the protected project open.
        if (choice.open && choice.outputPath)
          await projects.openPath(choice.outputPath);
      },
      { clearError: false },
    );
    if (!this.disposed) {
      await tick();
      if (!this.disposed && this.returnFocus?.isConnected)
        this.returnFocus.focus();
    }
  }
}
