import type { WorkspaceAction } from '../client';

interface WorkspaceEffects {
  activity: () => boolean;
  confirming: () => boolean;
  run: (action: WorkspaceAction, event?: Event) => Promise<void>;
}

/** The workspace owns admission; feature owners only expose their activity. */
export class Workspace {
  private tasks = $state(0);
  private alive = true;
  error = $state('');
  notice = $state('');
  ready = $state(false);

  constructor(
    private effects: WorkspaceEffects = {
      activity: () => false,
      confirming: () => false,
      run: async () => {},
    },
  ) {}
  // Wire the completed owner graph during initialization, before effects/mount.
  connect(effects: WorkspaceEffects) {
    this.effects = effects;
  }
  get disposed() {
    return !this.alive;
  }

  get busy(): boolean {
    return this.tasks > 0 || this.effects.activity();
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
    ++this.tasks;
    if (options.clearError !== false) this.error = '';
    try {
      await fn();
      return true;
    } catch (error) {
      if (this.alive) this.error = String(error);
      return false;
    } finally {
      --this.tasks;
    }
  }

  dispose() {
    this.alive = false;
  }
}
