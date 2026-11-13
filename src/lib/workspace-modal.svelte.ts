import type { Workspace } from './workspace.svelte';
export type AuthoringModal = 'styles' | 'fonts' | 'conversion' | 'plugins';

export type TimingKind = 'gap' | 'scene' | 'duration' | 'batch';
export type WorkspaceModalIdentity =
  | AuthoringModal
  | 'settings'
  | 'resync'
  | 'statistics'
  | 'text'
  | { kind: 'timing'; variant: TimingKind };

type Effects = {
  occlude: (value: boolean) => Promise<void>;
  remainingOcclusion: () => boolean;
  afterRender: () => Promise<void>;
  captureFocus: () => () => void;
};

/** Mount-scoped presentation only; feature sessions own acceptance and drafts. */
export class WorkspaceModal {
  identity = $state.raw<WorkspaceModalIdentity | null>(null);
  private phase = $state<'idle' | 'opening' | 'open' | 'closing'>('idle');
  private alive = true;
  private epoch = 0;
  private pendingHide: Promise<void> | null = null;
  private restoreFocus: (() => void) | null = null;

  constructor(
    private workspace: Pick<Workspace, 'error'>,
    private effects: Effects,
  ) {}

  get timingKind() {
    return this.identity !== null && typeof this.identity === 'object'
      ? this.identity.variant
      : null;
  }

  get opening() {
    return this.phase === 'opening';
  }
  get active() {
    return this.phase !== 'idle';
  }

  async open(identity: WorkspaceModalIdentity) {
    if (!this.alive || this.active) return;
    const epoch = ++this.epoch;
    this.phase = 'opening';
    this.restoreFocus = this.effects.captureFocus();
    try {
      const hide = this.effects.occlude(true);
      this.pendingHide = hide;
      await hide;
      if (!this.current(epoch)) return;
      this.identity = identity;
      this.phase = 'open';
    } catch (error) {
      if (this.current(epoch)) {
        this.phase = 'idle';
        this.restoreFocus = null;
        this.workspace.error = String(error);
      }
    } finally {
      if (this.current(epoch)) this.pendingHide = null;
    }
  }

  async close() {
    if (!this.alive || !this.active || this.phase === 'closing') return;
    const epoch = ++this.epoch;
    this.phase = 'closing';
    this.identity = null;
    try {
      // Closing during admission must restore after the outstanding hide, never
      // race it. Rejection is handled by the close transition below.
      await this.pendingHide;
      if (!this.current(epoch)) return;
      await this.effects.afterRender();
      if (!this.current(epoch)) return;
      await this.effects.occlude(this.effects.remainingOcclusion());
    } catch (error) {
      if (this.current(epoch)) this.workspace.error = String(error);
    } finally {
      if (this.current(epoch)) {
        this.pendingHide = null;
        this.phase = 'idle';
        const restore = this.restoreFocus;
        this.restoreFocus = null;
        // Toolbar enablement must render before restoring its focus.
        await this.effects.afterRender();
        if (this.current(epoch)) restore?.();
      }
    }
  }

  dispose() {
    this.alive = false;
    ++this.epoch;
    this.identity = null;
    this.phase = 'idle';
    this.restoreFocus = null;
    this.pendingHide = null;
    // Workspace media teardown owns native cleanup. No continuation may send
    // another command or focus a detached element after this mount ends.
  }

  private current(epoch: number) {
    return this.alive && this.epoch === epoch;
  }
}
