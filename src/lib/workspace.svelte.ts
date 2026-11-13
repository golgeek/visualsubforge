import type { WorkspaceAction } from './client';

interface WorkspaceEffects {
  activity: () => boolean;
  confirming: () => boolean;
  run: (action: WorkspaceAction, event?: Event) => Promise<void>;
}

/** The workspace owns admission; feature owners only expose their activity. */
export class Workspace {
  private working = $state(false);
  private alive = true;
  error = $state('');
  notice = $state('');
  ready = $state(false);

  constructor(private effects: WorkspaceEffects) {}

  get busy(): boolean {
    return this.working || this.effects.activity();
  }
  get confirming(): boolean {
    return this.effects.confirming();
  }
  get canAct(): boolean {
    return !this.busy && !this.confirming && this.ready;
  }
  run(action: WorkspaceAction, event?: Event) {
    return this.effects.run(action, event);
  }

  // Admission stays with the caller: some continuations run while a decision
  // is active. A false result lets a caller stop its next step after failure.
  async task(
    fn: () => Promise<unknown>,
    options: { clearError?: boolean } = {},
  ) {
    this.working = true;
    if (options.clearError !== false) this.error = '';
    try {
      await fn();
      return true;
    } catch (error) {
      if (this.alive) this.error = String(error);
      return false;
    } finally {
      if (this.alive) this.working = false;
    }
  }

  dispose() {
    this.alive = false;
  }
}
