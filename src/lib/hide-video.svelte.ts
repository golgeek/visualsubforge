export interface HideVideoEffects {
  occlude: (value: boolean) => Promise<void>;
  remainingOcclusion: () => boolean;
  afterRender: () => Promise<void>;
}

/** One hide/render/restore lifetime. Owners retain content, focus and answers. */
export class HideVideo {
  private phase = $state<'idle' | 'opening' | 'open' | 'closing'>('idle');
  private alive = true;
  private epoch = 0;
  private pendingHide: Promise<void> | null = null;
  constructor(private effects: HideVideoEffects) {}

  get disposed() {
    return !this.alive;
  }
  get active() {
    return this.phase !== 'idle';
  }
  get opening() {
    return this.phase === 'opening';
  }
  get visible() {
    return this.phase === 'open';
  }
  get closing() {
    return this.phase === 'closing';
  }

  async open(
    present: () => void,
    options: { replace?: boolean } = {},
  ): Promise<boolean> {
    if (!this.alive) return false;
    if (this.active && !options.replace)
      throw new Error('A video overlay is already active.');
    const previous = this.phase;
    const epoch = ++this.epoch;
    this.phase = 'opening';
    try {
      const hide = this.effects.occlude(true);
      this.pendingHide = hide;
      await hide;
      if (!this.current(epoch)) return false;
      present();
      this.phase = 'open';
      return true;
    } catch (error) {
      if (this.current(epoch)) {
        this.phase = previous === 'open' ? 'open' : 'idle';
        throw error;
      }
      return false;
    } finally {
      if (this.current(epoch)) this.pendingHide = null;
    }
  }

  async close(content: {
    hide: () => void;
    restored?: () => void;
    failed: (error: unknown) => void;
    focus?: () => void;
  }) {
    if (!this.alive || !this.active || this.closing) return;
    const epoch = ++this.epoch;
    this.phase = 'closing';
    content.hide();
    let restored = false;
    try {
      // A close during opening must follow its outstanding hide reply.
      await this.pendingHide;
      if (!this.current(epoch)) return;
      await this.effects.afterRender();
      if (!this.current(epoch)) return;
      await this.effects.occlude(this.effects.remainingOcclusion());
      restored = true;
    } catch (error) {
      if (this.current(epoch)) content.failed(error);
    } finally {
      if (this.current(epoch)) {
        this.pendingHide = null;
        this.phase = 'idle';
        if (restored) content.restored?.();
        if (content.focus) {
          // Controls must render their released gate before focus returns.
          await this.effects.afterRender();
          if (this.current(epoch)) content.focus();
        }
      }
    }
  }

  dispose() {
    this.alive = false;
    ++this.epoch;
    this.phase = 'idle';
    this.pendingHide = null;
    // Workspace media teardown owns native cleanup after disposal.
  }
  private current(epoch: number) {
    return this.alive && this.epoch === epoch;
  }
}
