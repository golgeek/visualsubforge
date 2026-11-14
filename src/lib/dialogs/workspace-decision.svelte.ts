import { tick } from 'svelte';
import type { MediaSession } from '../media/media-session.svelte';
import type { Workspace } from '../workspace/workspace.svelte';
import { PendingDecision } from './pending-decision';
import { HideVideo, type HideVideoEffects } from './hide-video.svelte';

interface DecisionEffects extends HideVideoEffects {
  close: () => void;
}

// Presentation and one answer, not save policy or the workspace gate.
export class WorkspaceDecision<Choice, Payload = never> {
  payload = $state.raw<Payload | null>(null);
  private video: HideVideo;
  private result: Promise<Choice> | null = null;
  private answering = false;
  private decision: PendingDecision<Choice>;
  constructor(
    private cancelled: Choice,
    protected workspace: Pick<Workspace, 'error'>,
    private effects: DecisionEffects,
  ) {
    this.video = new HideVideo(effects);
    this.decision = new PendingDecision(cancelled);
  }
  get active() {
    return this.video.opening || this.video.visible;
  }
  get visible() {
    return this.video.visible;
  }
  get closing() {
    return this.video.closing;
  }
  get disposed() {
    return this.video.disposed;
  }
  show(payload: Payload) {
    return this.open(() => {
      this.payload = payload;
    });
  }
  async request(payload: Payload): Promise<Choice> {
    return (await this.show(payload)) ? this.wait() : this.cancelled;
  }
  async ask(present: () => void): Promise<Choice> {
    return (await this.open(present)) ? this.wait() : this.cancelled;
  }
  // Imports release the action gate after open; save decisions await an answer.
  async open(present: () => void): Promise<boolean> {
    if (this.video.disposed) return false;
    if (this.video.active || this.decision.pending)
      throw new Error('A dialog decision is already pending.');
    // Unlike modals, admission failures propagate to the protected action.
    return this.video.open(() => {
      present();
      this.result = this.decision.wait();
    });
  }
  wait(): Promise<Choice> {
    return this.result ?? Promise.resolve(this.cancelled);
  }
  async answer(choice: Choice) {
    if (this.video.disposed || !this.decision.pending || this.answering) return;
    this.answering = true;
    await this.video.close({
      hide: () => {
        this.payload = null;
        this.effects.close();
      },
      restored: () => {
        this.decision.answer(choice);
      },
      failed: (error) => {
        this.workspace.error = String(error);
        this.decision.answer(this.cancelled);
      },
    });
    if (!this.video.disposed) this.answering = false;
  }
  dispose() {
    this.video.dispose();
    this.decision.dispose();
  }
}

export function decisionEffects(
  media: MediaSession,
  remainingOcclusion: () => boolean,
  close: () => void = () => {},
): DecisionEffects {
  return {
    occlude: (value) => media.occlude(value),
    remainingOcclusion,
    close,
    afterRender: tick,
  };
}
