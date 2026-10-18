import {
  translationAction,
  type TranslationReply,
  type TranslationKey,
  type DocumentSnapshot,
} from './client';
import type { DocumentSession } from './document-session.svelte';

type Job = NonNullable<TranslationReply['job']>;
type Input = {
  key: TranslationKey;
  targetCueId: number | null;
  offset: number;
  selected: number | null;
};
const working = (job: Job) =>
  ['working', 'accepting', 'cancelling'].includes(job.phase);
const wait = () => new Promise<void>((resolve) => setTimeout(resolve, 50));
export function referenceKey(document: DocumentSnapshot): TranslationKey {
  return {
    generation: document.generation,
    revision: document.revision,
    epoch: document.translation?.epoch ?? 0,
  };
}
function same(a: TranslationKey, b: TranslationKey) {
  return (
    a.generation === b.generation &&
    a.revision === b.revision &&
    a.epoch === b.epoch
  );
}

// Transient paging and one serial IPC lifetime. Rust owns every reference and
// accepted context; the latest desired query replaces obsolete interaction state.
export class TranslationSession {
  page = $state.raw<Job['page']>(null);
  private actionError = $state('');
  private queryError = $state('');
  get error() {
    return this.actionError || this.queryError;
  }
  loading = $state(false);
  querying = $state(false);
  private alive = true;
  private serial = 0;
  private id: number | null = null;
  private desired: Input | null = null;
  private desiredName = '';
  private published = '';
  private pumpTask: Promise<void> | null = null;
  private mutation = false;
  constructor(
    private document: DocumentSession,
    private send = translationAction,
  ) {}

  request(
    document: DocumentSnapshot,
    targetCueId: number | null,
    offset: number,
    selected: number | null,
    active: boolean,
  ) {
    const input =
      active && document.translation?.reference
        ? { key: referenceKey(document), targetCueId, offset, selected }
        : null;
    const name = input ? JSON.stringify(input) : '';
    if (name === this.desiredName) return;
    this.desiredName = name;
    this.desired = input;
    if (!this.mutation) ++this.serial;
    this.page = null;
    this.queryError = '';
    this.startPump();
  }
  private current(serial: number) {
    return this.alive && serial === this.serial;
  }
  private async retire(job?: Job | null) {
    const id = job?.id ?? this.id;
    if (id === null || id === undefined) return;
    let reply = await this.send({ kind: 'cancel', id });
    while (reply.job?.id === id && working(reply.job)) {
      await wait();
      reply = await this.send({ kind: 'view', id });
    }
    if (this.id === id) this.id = null;
  }
  private startPump() {
    if (this.pumpTask || this.mutation || !this.alive) return;
    this.pumpTask = this.pump().finally(() => {
      this.pumpTask = null;
      if (this.alive && !this.mutation && this.desiredName !== this.published)
        this.startPump();
    });
  }
  private async pump() {
    while (
      this.alive &&
      !this.mutation &&
      this.desiredName !== this.published
    ) {
      const serial = this.serial,
        input = this.desired,
        name = this.desiredName;
      this.querying = !!input;
      try {
        await this.retire();
        if (!this.current(serial)) continue;
        if (!input) {
          this.published = '';
          return;
        }
        const status = await this.send({ kind: 'status' });
        await this.retire(status.job);
        if (!this.current(serial)) continue;
        const reply = await this.send({ kind: 'query', ...input, limit: 100 });
        const job = reply.job;
        if (!job) throw new Error('Reference query returned no operation');
        this.id = job.id;
        const final = await this.finish(job, serial);
        if (!this.current(serial)) continue;
        if (!final || final.phase !== 'ready' || !same(final.key, input.key))
          throw new Error(
            final?.error ?? 'Reference query was cancelled or became stale',
          );
        this.page = final.page;
        this.published = name;
      } catch (error) {
        if (this.current(serial)) {
          this.queryError = String(error);
          this.published = name;
        }
      } finally {
        this.querying = false;
      }
    }
    if (!this.alive || !this.desired) await this.retire().catch(() => {});
  }
  private async finish(initial: Job, serial: number): Promise<Job | null> {
    let job: Job | null = initial;
    while (job && working(job) && this.current(serial)) {
      await wait();
      const reply = await this.send({ kind: 'view', id: initial.id });
      job = reply.job;
    }
    if (!this.current(serial)) {
      await this.retire(job);
      return null;
    }
    return job;
  }
  retry() {
    this.published = '';
    this.actionError = this.queryError = '';
    this.startPump();
  }
  cancel() {
    ++this.serial;
    this.page = null;
    this.published = this.desiredName;
    if (this.id !== null)
      void this.send({ kind: 'cancel', id: this.id }).catch(() => {});
  }
  private async change(
    operation: (
      key: TranslationKey,
      serial: number,
      expected: DocumentSnapshot,
    ) => Promise<boolean>,
  ) {
    if (!this.alive || this.mutation) return false;
    this.mutation = this.loading = true;
    const serial = ++this.serial;
    this.page = null;
    this.actionError = this.queryError = '';
    const expected = this.document.snapshot;
    try {
      if (this.pumpTask) await this.pumpTask;
      const status = await this.send({ kind: 'status' });
      if (!this.current(serial)) return false;
      await this.retire(status.job);
      if (!this.current(serial)) return false;
      if (
        this.document.snapshot !== expected ||
        status.key.generation !== expected.generation ||
        status.key.revision !== expected.revision ||
        (expected.loaded && !same(status.key, referenceKey(expected)))
      )
        throw new Error('The target or reference changed; try again.');
      return await operation(status.key, serial, expected);
    } catch (error) {
      if (this.alive) this.actionError = String(error);
      return false;
    } finally {
      this.loading = this.mutation = false;
      this.published = '';
      if (this.alive) this.startPump();
    }
  }
  load(path: string, copy: boolean) {
    return this.change(async (key, serial, expected) => {
      const reply = await this.send({ kind: 'load', key, path, copy });
      if (!reply.job) throw new Error('Reference load returned no operation');
      this.id = reply.job.id;
      const ready = await this.finish(reply.job, serial);
      if (!this.current(serial)) return false;
      if (ready?.phase !== 'ready')
        throw new Error(
          ready?.error ?? 'Reference load was cancelled or became stale',
        );
      // App protects replacement before a copy load; an attachment preserves drafts.
      const accepted = await this.document.changeTranslation(
        { kind: 'accept', id: ready.id, discard: copy },
        expected,
      );
      this.id = null;
      return accepted;
    });
  }
  setOffset(offsetMs: number) {
    return this.change((key, _serial, expected) =>
      this.document.changeTranslation(
        { kind: 'setOffset', key, offsetMs },
        expected,
      ),
    );
  }
  detach() {
    return this.change((key, _serial, expected) =>
      this.document.changeTranslation({ kind: 'detach', key }, expected),
    );
  }
  dispose() {
    this.alive = false;
    this.desired = null;
    this.desiredName = '';
    this.cancel();
    void this.retire().catch(() => {});
  }
}
