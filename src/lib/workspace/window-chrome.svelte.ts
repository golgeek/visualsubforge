import {
  dragWindow,
  minimizeWindow,
  toggleWindowMaximized,
  setWindowDecorations,
  windowIsDecorated,
  windowPlatform,
  isTauri,
} from '../client';
import { HideVideo, type HideVideoEffects } from '../dialogs/hide-video.svelte';
import type { Workspace } from './workspace.svelte';
const client = {
  dragWindow,
  minimizeWindow,
  toggleWindowMaximized,
  setWindowDecorations,
  windowIsDecorated,
};
/** Native captions by default; Linux's saved opt-in is read once per mount. */
export class WindowChrome {
  custom = $state(false);
  private started = false;
  private video: HideVideo;
  constructor(
    private workspace: Workspace,
    effects: HideVideoEffects,
    readonly platform = windowPlatform(),
    readonly native = isTauri(),
    private api = client,
  ) {
    this.video = new HideVideo(effects);
  }
  get overlay() {
    return this.native && (this.platform === 'macos' || this.custom);
  }
  get menus() {
    return (
      this.native && (this.platform === 'windows' || this.platform === 'linux')
    );
  }
  async start(custom: boolean) {
    if (this.started || this.video.disposed) return;
    this.started = true;
    if (!this.native || this.platform !== 'linux' || !custom) return;
    await this.resize(async () => {
      try {
        await this.api.setWindowDecorations(false);
        if (!this.video.disposed) this.custom = true;
      } catch (error) {
        // An unknown write outcome must not leave a borderless window without controls.
        if (!this.video.disposed) this.custom = true;
        try {
          const decorated = await this.api.windowIsDecorated();
          if (!this.video.disposed) this.custom = !decorated;
        } catch {
          /* Keep the reported native error; never replay the mutation. */
        }
        throw error;
      }
    });
  }
  private async resize(operation: () => Promise<void>) {
    if (
      !this.native ||
      this.video.disposed ||
      this.video.active ||
      !this.workspace.canAct
    )
      return;
    await this.workspace.task(async () => {
      try {
        if (await this.video.open(() => {})) await operation();
      } finally {
        await this.video.close({
          restoreWhenIdle: true,
          hide: () => {},
          failed: (error) => {
            this.workspace.error = String(error);
          },
        });
      }
    });
  }
  async drag(event: MouseEvent) {
    if (
      !this.overlay ||
      this.video.disposed ||
      event.button !== 0 ||
      event.detail !== 1
    )
      return;
    const target = event.target;
    if (
      target instanceof Element &&
      !target.closest('[data-window-drag]') &&
      target.closest('button, input, select, a, label')
    )
      return;
    await this.workspace.task(() => this.api.dragWindow(), {
      clearError: false,
    });
  }
  minimize() {
    if (!this.native || this.video.disposed) return;
    return this.workspace.task(() => this.api.minimizeWindow(), {
      clearError: false,
    });
  }
  maximize() {
    return this.resize(() => this.api.toggleWindowMaximized());
  }
  dispose() {
    this.video.dispose();
  }
}
