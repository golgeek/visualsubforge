import { untrack } from 'svelte';
import {
  statisticsStart,
  statisticsView,
  statisticsCancel,
  type StatisticsView,
  type StatisticsSelection,
} from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
const client = { statisticsStart, statisticsView, statisticsCancel };
// Read-only projection. Drafts never enter statistics and remain owned by the
// document session. Rust owns the report and validates every context on reads.
export class StatisticsSession {
  view = $state.raw<StatisticsView | null>(null);
  error = $state('');
  pending = $state(false);
  private alive = true;
  private epoch = 0;
  private id: number | null = null;
  private polling = false;
  private offset = 0;
  private capture = $state.raw<{
    generation: number;
    revision: number;
    session: number;
    context: string;
    scope: string;
  } | null>(null);
  private timer: ReturnType<typeof setTimeout> | null = null;
  private liveKey = '';
  private refresh = false;
  private attempts = 0;
  constructor(
    private subtitles: DocumentSession,
    private mediaSession: () => number,
    private context: () => string,
    private input: () => { key: string; selection: StatisticsSelection | null },
    private api = client,
    private live?: () => boolean,
  ) {
    if (live)
      $effect(() => {
        const key = JSON.stringify([
          subtitles.snapshot.generation,
          subtitles.snapshot.revision,
          mediaSession(),
          context(),
          input().key,
          live(),
        ]);
        untrack(() => {
          if (key === this.liveKey) return;
          if (key !== this.liveKey) {
            this.liveKey = key;
            this.refresh = true;
            this.attempts = 0;
            this.error = '';
          }
          this.schedule(250);
        });
      });
  }
  private schedule(delay: number) {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.drive();
    }, delay);
  }
  private async drive() {
    if (!this.alive || !this.live) return;
    if (this.pending || this.polling) {
      this.schedule(100);
      return;
    }
    const key = this.liveKey;
    if (!this.live() || this.refresh) {
      if (
        this.id !== null &&
        (!this.view ||
          !['cancelled', 'stale', 'failed', 'closed'].includes(this.view.phase))
      ) {
        await this.cancel();
        if (this.alive) this.schedule(250);
        return;
      }
      if (!this.live()) return;
      if (!this.input().selection) {
        this.refresh = false;
        return;
      }
      await this.start();
      if (!this.alive) return;
      if (key !== this.liveKey) {
        this.schedule(250);
        return;
      }
      if (this.id === null) {
        if (++this.attempts < 3) this.schedule(250);
        else this.refresh = false;
        return;
      }
      this.refresh = false;
    } else await this.poll();
    if (this.alive && this.id !== null) this.schedule(400);
  }
  get current() {
    const s = this.subtitles.snapshot;
    return (
      !!this.capture &&
      s.generation === this.capture.generation &&
      s.revision === this.capture.revision &&
      this.mediaSession() === this.capture.session &&
      this.context() === this.capture.context &&
      this.input().key === this.capture.scope
    );
  }
  get running() {
    return (
      this.id !== null &&
      (!this.view || ['preparing', 'cancelling'].includes(this.view.phase))
    );
  }
  get canStart() {
    return (
      this.alive &&
      !this.pending &&
      !this.running &&
      this.input().selection !== null
    );
  }
  private active(epoch: number) {
    return this.alive && this.epoch === epoch;
  }
  async start() {
    const input = this.input();
    if (!this.canStart || !input.selection) return;
    const document = this.subtitles.snapshot;
    const capture = {
      generation: document.generation,
      revision: document.revision,
      session: this.mediaSession(),
      context: this.context(),
      scope: input.key,
    };
    const epoch = ++this.epoch;
    this.pending = true;
    this.error = '';
    this.view = null;
    this.offset = 0;
    try {
      if (this.id !== null) await this.api.statisticsCancel(this.id);
      if (!this.active(epoch)) return;
      this.id = null;
      this.capture = capture;
      const id = await this.api.statisticsStart(
        capture.session,
        document,
        input.selection,
      );
      if (!this.active(epoch)) {
        await this.api.statisticsCancel(id).catch(() => {});
        return;
      }
      this.id = id;
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) {
        if (this.id !== null)
          void this.api.statisticsCancel(this.id).catch(() => {});
        this.error = String(e);
        this.id = null;
        this.capture = null;
      }
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  private async read(epoch: number) {
    if (this.id === null) return;
    const view = await this.api.statisticsView(this.id, this.offset, 100);
    if (!this.active(epoch)) return;
    if (
      !this.current ||
      view.id !== this.id ||
      view.key.generation !== this.capture?.generation ||
      view.key.revision !== this.capture?.revision ||
      view.session !== this.capture?.session
    ) {
      await this.api.statisticsCancel(this.id).catch(() => {});
      if (!this.active(epoch)) return;
      this.view = null;
      this.id = null;
      this.capture = null;
      this.error = 'Scope or workspace changed; calculate statistics again.';
      return;
    }
    this.view = view;
    if (view.error) this.error = view.error;
  }
  async poll() {
    if (!this.alive || this.pending || this.polling || this.id === null) return;
    this.polling = true;
    const epoch = this.epoch;
    try {
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) {
        if (this.id !== null)
          void this.api.statisticsCancel(this.id).catch(() => {});
        this.error = String(e);
        this.view = null;
        this.id = null;
        this.capture = null;
      }
    } finally {
      this.polling = false;
    }
  }
  async page(offset: number) {
    if (!this.canStart || !this.current || this.view?.phase !== 'ready') return;
    this.offset = offset;
    this.pending = true;
    const epoch = ++this.epoch;
    try {
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) this.error = String(e);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  async cancel() {
    if (!this.alive || this.pending || this.id === null) return;
    const epoch = ++this.epoch;
    this.pending = true;
    try {
      await this.api.statisticsCancel(this.id);
      await this.read(epoch);
    } catch (e) {
      if (this.active(epoch)) this.error = String(e);
    } finally {
      if (this.active(epoch)) this.pending = false;
    }
  }
  dispose() {
    if (!this.alive) return;
    this.alive = false;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    ++this.epoch;
    if (this.id !== null)
      void this.api.statisticsCancel(this.id).catch(() => {});
  }
}
