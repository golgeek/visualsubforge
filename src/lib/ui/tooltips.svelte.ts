import { HideVideo, type HideVideoEffects } from '../dialogs/hide-video.svelte';
import { actionDescription, type ActionId } from '../workspace/action-registry';

export const tooltipContext = Symbol('workspace tooltips');
/** One delayed tooltip at a time; native video follows the shared overlay lifetime. */
export class Tooltips {
  content = $state<{ id: ActionId; x: number; y: number } | null>(null);
  private video: HideVideo;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private epoch = 0;
  private closing: Promise<void> | null = null;
  private alive = true;
  constructor(
    effects: HideVideoEffects,
    private failure: (error: unknown) => void,
    private available: () => boolean = () => true,
  ) {
    this.video = new HideVideo(effects);
  }
  get active() {
    return this.video.active;
  }
  show(id: ActionId, anchor: HTMLElement) {
    const epoch = ++this.epoch;
    this.cancelTimer();
    if (!this.alive || !this.available()) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.present(epoch, id, anchor);
    }, 350);
  }
  private async present(epoch: number, id: ActionId, anchor: HTMLElement) {
    await this.close();
    if (
      !this.alive ||
      !this.available() ||
      epoch !== this.epoch ||
      !anchor.isConnected
    )
      return;
    const bounds = anchor.getBoundingClientRect();
    try {
      await this.video.open(() => {
        this.content = {
          id,
          x: Math.max(8, Math.min(bounds.left, window.innerWidth - 280)),
          y: Math.max(8, Math.min(bounds.bottom + 6, window.innerHeight - 64)),
        };
      });
    } catch (error) {
      if (this.alive) this.failure(error);
    }
  }
  hide() {
    ++this.epoch;
    this.cancelTimer();
    return this.close();
  }
  private close() {
    if (this.closing) return this.closing;
    const pending = this.video.close({
      hide: () => {
        this.content = null;
      },
      failed: this.failure,
    });
    this.closing = pending;
    void pending.finally(() => {
      if (this.closing === pending) this.closing = null;
    });
    return pending;
  }
  private cancelTimer() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
  dispose() {
    this.alive = false;
    ++this.epoch;
    this.cancelTimer();
    this.video.dispose();
    this.content = null;
  }
  description(id: ActionId) {
    return actionDescription(id);
  }
}
