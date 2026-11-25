import { actions, unavailable } from './action-registry';
import type { WorkspaceCommands } from './workspace-commands';
import { workspaceActions, updateMenu } from '../client';
import type { Workspace } from './workspace.svelte';
export class WorkspaceMenus {
  private alive = true;
  private lastMenu = '';
  private unlisten = () => {};
  constructor(
    private workspace: Workspace,
    private commands: WorkspaceCommands,
  ) {
    $effect(() => {
      if (!this.alive) return;
      const context = this.commands.context;
      const state = {
        splitInHalfReason: unavailable('splitInHalf', context),
        enabled: actions
          .filter(
            (action) =>
              action.native &&
              !this.workspace.confirming &&
              !unavailable(action.id, context),
          )
          .map((action) => action.id),
        checked: Object.entries(context.checks ?? {})
          .filter(([, checked]) => checked)
          .map(([id]) => id),
      };
      const key = JSON.stringify(state);
      if (key === this.lastMenu) return;
      this.lastMenu = key;
      void updateMenu(state).catch((e) => {
        if (this.alive) this.workspace.error = String(e);
      });
    });
  }
  startListening() {
    void workspaceActions((action) => {
      if (this.alive) void this.workspace.run(action);
    })
      .then((stop) => {
        if (this.alive) this.unlisten = stop;
        else stop();
      })
      .catch((error) => {
        if (this.alive) this.workspace.error = String(error);
      });
  }
  dispose() {
    this.alive = false;
    this.unlisten();
  }
}
