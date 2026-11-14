import { untrack } from 'svelte';
import {
  cueFixesStart,
  cueFixesView,
  cueFixesCancel,
  type CueFixesView,
  type DocumentSnapshot,
} from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
interface Input {
  document: DocumentSnapshot;
  cueId: number;
  session: number;
  context: string;
}
const api = {
  start: cueFixesStart,
  view: cueFixesView,
  cancel: cueFixesCancel,
};
/** Projection of one Rust plan. The shared timing worker owns computation;
 * DocumentSession alone adopts acceptance. No arithmetic or history here. */
export class CueFixes {
  view = $state.raw<CueFixesView | null>(null);
  error = $state('');
  preparing = $state(false);
  accepting = $state(false);
  private alive = true;
  private sequence = 0;
  private identity = '';
  private input: Input | null = null;
  private id: number | null = null;
  private pending = false;
  private timer: ReturnType<typeof setTimeout> | null = null;
  constructor(
    private read: () => Input | null,
    private subtitles: Pick<DocumentSession, 'acceptCueFixes'>,
    private client = api,
  ) {
    $effect(() => {
      const input = this.read();
      const accepting = this.accepting;
      untrack(() => {
        if (!accepting) this.configure(input);
      });
    });
  }
  private configure(input: Input | null) {
    const identity = input
      ? JSON.stringify([
          input.document.generation,
          input.document.revision,
          input.cueId,
          input.session,
          input.context,
        ])
      : '';
    if (identity === this.identity) return;
    this.identity = identity;
    this.input = input;
    ++this.sequence;
    this.clearTimer();
    if (this.id !== null) void this.client.cancel(this.id).catch(() => {});
    this.id = null;
    this.view = null;
    this.error = '';
    this.preparing = !!input;
    if (input) this.schedule(350);
  }
  private clearTimer() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
  private schedule(delay: number) {
    this.clearTimer();
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.poll();
    }, delay);
  }
  private async poll() {
    const input = this.input;
    if (!this.alive || !input || this.pending || this.accepting) return;
    const sequence = this.sequence;
    this.pending = true;
    try {
      if (this.id === null) {
        const id = await this.client.start(
          input.session,
          input.document,
          input.cueId,
        );
        if (!this.alive || sequence !== this.sequence) {
          await this.client.cancel(id).catch(() => {});
          return;
        }
        this.id = id;
      }
      const view = await this.client.view(this.id);
      if (!this.alive || sequence !== this.sequence) return;
      this.view = view;
      this.preparing = view.phase === 'preparing';
      this.error = view.error ?? '';
      if (this.preparing) this.schedule(200);
    } catch (error) {
      if (this.alive && sequence === this.sequence) {
        this.error = String(error);
        this.preparing = false;
      }
    } finally {
      this.pending = false;
      if (this.alive && sequence !== this.sequence && this.input)
        this.schedule(350);
    }
  }
  retry() {
    if (!this.alive || this.accepting || !this.input) return;
    ++this.sequence;
    if (this.id !== null) void this.client.cancel(this.id).catch(() => {});
    this.id = null;
    this.view = null;
    this.error = '';
    this.preparing = true;
    this.schedule(350);
  }
  get count() {
    return this.view?.phase === 'reviewing' && !this.error
      ? this.view.fixed
      : 0;
  }
  async accept() {
    const input = this.input;
    if (
      !this.alive ||
      !input ||
      !this.count ||
      this.id === null ||
      this.accepting
    )
      return false;
    const current = this.read();
    if (
      !current ||
      current.document !== input.document ||
      current.cueId !== input.cueId ||
      current.context !== input.context ||
      current.session !== input.session
    )
      return false;
    this.accepting = true;
    try {
      const accepted = await this.subtitles.acceptCueFixes(
        this.id,
        input.document,
      );
      return this.alive && accepted;
    } catch (error) {
      if (this.alive) this.error = String(error);
      throw error;
    } finally {
      if (this.alive) this.accepting = false;
    }
  }
  dispose() {
    this.alive = false;
    ++this.sequence;
    this.clearTimer();
    if (this.id !== null) void this.client.cancel(this.id).catch(() => {});
  }
}
