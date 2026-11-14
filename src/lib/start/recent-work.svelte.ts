import {
  listRecent,
  captureRecent,
  forgetRecent,
  type RecentEntry,
  type RecentRequest,
} from '../client';
const client = { listRecent, captureRecent, forgetRecent };
interface Input {
  identity: string;
  request: RecentRequest;
}
/** Historical projection only. Rust captures paths, source identity and checker counts. */
export class RecentWork {
  entries = $state.raw<RecentEntry[]>([]);
  error = $state('');
  loading = $state(false);
  removing = $state(false);
  private alive = true;
  private sequence = 0;
  private loadSequence = 0;
  private input: Input | null = null;
  private attempted: string | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private pending: Promise<void> | null = null;
  constructor(private api = client) {}
  async load() {
    if (!this.alive) return;
    const loadSequence = ++this.loadSequence;
    const sequence = ++this.sequence;
    this.loading = true;
    try {
      const entries = await this.api.listRecent();
      if (this.alive && sequence === this.sequence) {
        this.entries = entries;
        this.error = '';
      }
    } catch (error) {
      if (this.alive && sequence === this.sequence) this.error = String(error);
    } finally {
      if (this.alive && loadSequence === this.loadSequence)
        this.loading = false;
    }
  }
  configure(input: Input | null) {
    if (!this.alive) return;
    const changed = input?.identity !== this.input?.identity;
    this.input = input;
    if (!changed) return;
    ++this.sequence;
    this.clearTimer();
    this.schedule();
  }
  private clearTimer() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
  private schedule() {
    if (
      !this.alive ||
      this.pending ||
      this.removing ||
      !this.input ||
      this.input.identity === this.attempted
    )
      return;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.capture();
    }, 500);
  }
  private async capture() {
    const input = this.input;
    if (!this.alive || !input || this.pending) return;
    const sequence = ++this.sequence;
    this.attempted = input.identity;
    this.pending = (async () => {
      try {
        const entries = await this.api.captureRecent(input.request);
        if (this.alive && sequence === this.sequence) {
          this.entries = entries;
          this.error = '';
        }
      } catch (error) {
        if (this.alive && sequence === this.sequence)
          this.error = String(error);
      }
    })();
    try {
      await this.pending;
    } finally {
      this.pending = null;
      this.schedule();
    }
  }
  async forget(entry: RecentEntry) {
    if (!this.alive || this.removing) return;
    this.removing = true;
    this.clearTimer();
    await this.pending;
    if (!this.alive) return;
    const sequence = ++this.sequence;
    try {
      const entries = await this.api.forgetRecent(entry);
      if (this.alive && sequence === this.sequence) {
        this.entries = entries;
        this.error = '';
      }
    } catch (error) {
      if (this.alive && sequence === this.sequence) this.error = String(error);
    } finally {
      if (this.alive) {
        this.removing = false;
        this.schedule();
      }
    }
  }
  dispose() {
    this.alive = false;
    ++this.sequence;
    this.clearTimer();
    this.input = null;
  }
}
