import {
  emptyStatus,
  mediaAction,
  type MediaAction,
  type MediaStatus,
} from './client';
import { LiveLoop } from './live-loop';

// The native engine owns playback. This owner publishes only current responses
// and owns the polling/loop lifetimes; DOM geometry and gestures stay in App.
export class MediaSession {
  private value = $state.raw<MediaStatus>({ ...emptyStatus });
  private waiting = $state(false);
  private alive = true;
  private revision = 0;
  private polling = false;
  private timer: ReturnType<typeof setInterval> | null = null;
  private send: typeof mediaAction;
  private failed: (error: unknown) => void;
  readonly loop: LiveLoop;

  constructor(failed: (error: unknown) => void, send = mediaAction) {
    this.send = send;
    this.failed = failed;
    this.loop = new LiveLoop(
      async (session, [start, end]) => {
        if (this.alive) await this.send({ kind: 'loop', start, end }, session);
      },
      (error) => {
        if (this.alive) failed(`Loop preview: ${String(error)}`);
      },
    );
  }
  get status() {
    return this.value;
  }
  get pending() {
    return this.waiting;
  }
  private current(request: number) {
    return this.alive && request === this.revision;
  }

  async command(action: MediaAction): Promise<boolean> {
    if (!this.alive) return false;
    const request = ++this.revision;
    this.waiting = true;
    try {
      if (['open', 'close', 'clearLoop'].includes(action.kind))
        await this.loop.stop();
      else await this.loop.flush();
      if (!this.current(request)) return false;
      const next = await this.send(action, this.value.session);
      if (!this.current(request)) return false;
      this.value = next;
      if (action.kind === 'loop') this.loop.activate(next.session);
      return true;
    } catch (error) {
      if (this.current(request)) throw error;
      return false;
    } finally {
      if (this.current(request)) this.waiting = false;
    }
  }
  async poll() {
    if (!this.alive || this.polling || this.waiting) return;
    this.polling = true;
    const request = this.revision;
    try {
      const next = await this.send({ kind: 'status' }, this.value.session);
      if (this.current(request)) this.value = next;
    } catch (error) {
      if (this.current(request)) this.failed(error);
    } finally {
      this.polling = false;
    }
  }
  startPolling() {
    if (!this.alive || this.timer !== null) return;
    this.timer = setInterval(() => void this.poll(), 100);
    void this.poll();
  }
  async waitUntilLoaded(session: number, timeout = 15_000) {
    // loadfile ACK precedes FILE_LOADED. Normal polling advances readiness;
    // replacement/disposal stop this wait and never authorize a later seek.
    const deadline = performance.now() + timeout;
    while (
      this.alive &&
      this.value.session === session &&
      !this.value.loaded &&
      !this.value.error
    ) {
      if (performance.now() >= deadline)
        throw new Error('Timed out waiting for project video to load');
      await new Promise<void>((resolve) => setTimeout(resolve, 50));
    }
    if (!this.alive || this.value.session !== session)
      throw new Error('Media changed while opening project');
    if (this.value.error) throw new Error(this.value.error);
  }
  async occlude(occluded: boolean) {
    if (this.alive && this.value.available && this.value.surfaceAboveWebview)
      await this.send({ kind: 'occlude', occluded }, this.value.session);
  }
  async layout(action: Extract<MediaAction, { kind: 'layout' }>) {
    if (this.alive) await this.send(action, this.value.session);
  }
  dispose() {
    this.alive = false;
    ++this.revision;
    if (this.timer !== null) clearInterval(this.timer);
    this.timer = null;
    void this.loop.stop();
  }
}
