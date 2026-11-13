import type { Workspace } from './workspace.svelte';
import type { MediaSession } from './media-session.svelte';
/** One native-layout request at a time; disposal removes every observer/listener/timer. */
export class NativeVideoLayout {
  private alive = true;
  private pending = false;
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
      if (!this.alive || this.pending || !this.media.status.available) return;
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
      this.pending = true;
      try {
        await this.media.layout(bounds);
        if (this.alive) this.last = key;
      } catch (e) {
        if (this.alive) this.workspace.error = String(e);
      } finally {
        this.pending = false;
      }
    };
    this.observer = new ResizeObserver(() => void this.update());
    this.observer.observe(video);

    window.addEventListener('resize', this.update);
    window.addEventListener('scroll', this.update, true);
    this.timer = setInterval(() => void this.update(), 100);
  }
  dispose() {
    this.alive = false;
    this.observer?.disconnect();
    if (this.timer !== null) clearInterval(this.timer);
    window.removeEventListener('resize', this.update);
    window.removeEventListener('scroll', this.update, true);
  }
}
