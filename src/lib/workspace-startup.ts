import { defaultPreferences, getPreferences } from './client';
import type { Workspace } from './workspace.svelte';
import type { DocumentSession } from './document-session.svelte';
import type { WorkspacePreferences } from './workspace-preferences.svelte';
export class WorkspaceStartup {
  private alive = true;
  constructor(
    private workspace: Workspace,
    private subtitles: DocumentSession,
    private preferences: WorkspacePreferences,
  ) {}
  start() {
    void Promise.all([
      this.subtitles.command({ kind: 'status' }),
      getPreferences().catch((e) => {
        if (this.alive) this.workspace.error = String(e);
        return defaultPreferences;
      }),
    ])
      .then(([, saved]) => {
        if (this.alive) {
          this.preferences.initialize(saved);
        }
      })
      .catch((e) => {
        if (this.alive) this.workspace.error = String(e);
      })
      .finally(() => {
        if (this.alive) this.workspace.ready = true;
      });
  }
  dispose() {
    this.alive = false;
  }
}
