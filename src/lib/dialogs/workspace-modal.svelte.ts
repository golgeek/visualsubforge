import { HideVideo, type HideVideoEffects } from './hide-video.svelte';
import type { Workspace } from '../workspace/workspace.svelte';
export type AuthoringModal = 'styles' | 'fonts' | 'conversion' | 'plugins';

export type TimingKind = 'gap' | 'scene' | 'duration' | 'batch';
export type WorkspaceModalIdentity =
  | AuthoringModal
  | 'settings'
  | 'resync'
  | 'statistics'
  | 'commands'
  | 'shortcuts'
  | 'actionMenu'
  | 'layout'
  | 'columns'
  | 'cueIssues'
  | 'text'
  | { kind: 'timing'; variant: TimingKind };

type Effects = HideVideoEffects & { captureFocus: () => () => void };

/** Mount-scoped presentation only; feature sessions own acceptance and drafts. */
export class WorkspaceModal {
  identity = $state.raw<WorkspaceModalIdentity | null>(null);
  private video: HideVideo;
  private restoreFocus: (() => void) | null = null;
  constructor(
    private workspace: Pick<Workspace, 'error'>,
    private effects: Effects,
  ) {
    this.video = new HideVideo(effects);
  }
  get timingKind() {
    return this.identity !== null && typeof this.identity === 'object'
      ? this.identity.variant
      : null;
  }
  get opening() {
    return this.video.opening;
  }
  get active() {
    return this.video.active;
  }

  async open(identity: WorkspaceModalIdentity) {
    if (this.video.disposed || this.active) return;
    this.restoreFocus = this.effects.captureFocus();
    try {
      await this.video.open(() => {
        this.identity = identity;
      });
    } catch (error) {
      if (!this.video.disposed) {
        this.restoreFocus = null;
        this.workspace.error = String(error);
      }
    }
  }
  close() {
    return this.video.close({
      hide: () => {
        this.identity = null;
      },
      failed: (error) => {
        this.workspace.error = String(error);
      },
      focus: () => {
        const restore = this.restoreFocus;
        this.restoreFocus = null;
        restore?.();
      },
    });
  }
  dispose() {
    this.video.dispose();
    this.identity = null;
    this.restoreFocus = null;
  }
}
