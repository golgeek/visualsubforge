import type { Workspace } from '../workspace/workspace.svelte';
import type { MediaSession } from './media-session.svelte';
/** One native-layout request at a time; disposal removes every observer/listener/timer. */
export class NativeVideoLayout {
  private alive = true;
  private pending: Promise<void> | null = null;
  private last = '';
  private observer: ResizeObserver | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private update: () => Promise<void> = async () => {};
  constructor(
    private workspace: Pick<Workspace, 'error'>,
    private media: Pick<MediaSession, 'status' | 'layout'>,
  ) {}
  mount(video: HTMLDivElement) {
    this.update = async () => {
      if (!this.alive || !this.media.status.available) return;
      if (this.pending) return this.pending;
      const rect = video.getBoundingClientRect();
      const bounds = {
        kind: 'layout' as const,
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        visible:
          this.media.status.loaded &&
          rect.bottom > 0 &&
          rect.right > 0 &&
          rect.top < innerHeight &&
          rect.left < innerWidth,
      };
      const key = JSON.stringify([
        bounds,
        innerWidth,
        innerHeight,
        devicePixelRatio,
      ]);
      if (key === this.last) return;
      const pending = this.media
        .layout(bounds)
        .then(() => {
          if (this.alive) this.last = key;
        })
        .catch((error: unknown) => {
          if (this.alive) this.workspace.error = String(error);
          throw error;
        });
      this.pending = pending;
      try {
        await pending;
      } finally {
        if (this.pending === pending) this.pending = null;
      }
    };
    this.observer = new ResizeObserver(this.request);
    this.observer.observe(video);

    window.addEventListener('resize', this.request);
    window.addEventListener('scroll', this.request, true);
    this.timer = setInterval(this.request, 100);
  }
  private request = () => {
    void this.update().catch(() => {});
  };
  /** Drain an old position, then acknowledge the currently rendered placeholder. */
  async flush() {
    await this.pending;
    if (this.alive) await this.update();
  }
  dispose() {
    this.alive = false;
    this.observer?.disconnect();
    if (this.timer !== null) clearInterval(this.timer);
    window.removeEventListener('resize', this.request);
    window.removeEventListener('scroll', this.request, true);
  }
}
