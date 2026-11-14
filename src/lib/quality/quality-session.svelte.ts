/* eslint-disable svelte/prefer-svelte-reactivity -- Collections are replaced as immutable projections; the private metric cache is not reactive state. */
import { addIssuePage, type IssueSeverity } from './cue-issues';
import {
  readQuality,
  cancelQuality,
  type QualityRequest,
  type QualityView,
  type QualityDraft,
} from '../client';

export interface QualityInput {
  session: number;
  enabled: boolean;
  generation: number;
  revision: number;
  context: string;
  selected: number | null;
  metricsOffset: number;
  metricsLimit: number;
  draft: QualityDraft | null;
  indexIssues?: boolean;
  metricOffsets?: number[];
}
const client = { readQuality, cancelQuality };

// A bounded projection, UI filters and drafts only. Rust owns reports and worker
// admission. One in-flight read avoids unordered draft updates or polling queues.
export class QualitySession {
  view = $state.raw<QualityView | null>(null);
  error = $state('');
  stale = $state(true);
  paused = $state(false);
  rule = $state<QualityRequest['rule']>(null);
  severity = $state<QualityRequest['severity']>(null);
  offset = $state(0);
  issues = $state.raw<ReadonlyMap<string, IssueSeverity>>(new Map());
  metrics = $state.raw<ReadonlyMap<string, QualityView['metrics'][number]>>(
    new Map(),
  );
  issueIndexComplete = $state(false);
  projectionError = $state('');
  private reportKey = '';
  private findingOffset = 0;
  private metricWindows = new Map<number, QualityView['metrics']>();
  private input: QualityInput | null = null;
  private signature = '';
  private version = '';
  private sequence = 0;
  private id: number | null = null;
  private alive = true;
  private pending = false;
  private restartPending = false;
  private draftSignature: string | undefined;
  private epoch: number | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  constructor(private api = client) {}

  configure(input: QualityInput) {
    if (!this.alive) return;
    const signature = JSON.stringify([
      input,
      this.rule,
      this.severity,
      this.offset,
    ]);
    if (signature === this.signature) return;
    const version = `${input.session}/${input.generation}/${input.revision}/${input.context}`;
    if (version !== this.version) {
      this.version = version;
      this.view = null;
      this.clearProjection();
      this.draftSignature = undefined;
      this.epoch = null;
      this.offset = 0;
    }
    this.signature = signature;
    this.input = input;
    ++this.sequence;
    this.stale = true;
    if (!input.enabled) {
      this.clearTimer();
      this.view = null;
      this.clearProjection();
      this.draftSignature = undefined;
      if (this.id !== null)
        void this.api.cancelQuality(this.id).catch(() => {});
      this.restartPending = true;
    } else this.schedule(180);
  }
  private clearProjection() {
    this.reportKey = '';
    this.findingOffset = 0;
    this.issues = new Map();
    this.metrics = new Map();
    this.metricWindows.clear();
    this.issueIndexComplete = false;
    this.projectionError = '';
  }
  private reportIdentity(view: QualityView) {
    return JSON.stringify([view.id, view.key]);
  }
  private keepMetrics(offset: number, rows: QualityView['metrics']) {
    this.metricWindows.delete(offset);
    this.metricWindows.set(offset, rows);
    // Bound retained windows even when a filter selects widely separated cues.
    while (this.metricWindows.size > 32)
      this.metricWindows.delete(this.metricWindows.keys().next().value!);
    this.metrics = new Map(
      [...this.metricWindows.values()].flatMap((page) =>
        page.map((metric) => [metric.data.cueId, metric] as const),
      ),
    );
  }
  private missingMetricWindow() {
    return this.input?.metricOffsets
      ?.slice(0, 32)
      .find((offset) => !this.metricWindows.has(offset));
  }
  private get projectionPending() {
    return (
      !!this.input?.indexIssues &&
      !this.paused &&
      !this.projectionError &&
      (!this.issueIndexComplete || this.missingMetricWindow() !== undefined)
    );
  }
  private async project(
    view: QualityView,
    request: QualityRequest,
    version: string,
  ) {
    if (!this.input?.indexIssues || view.phase !== 'ready' || this.paused)
      return;
    const identity = this.reportIdentity(view);
    if (identity !== this.reportKey) {
      this.clearProjection();
      this.reportKey = identity;
    }
    if (request.metricsLimit)
      this.keepMetrics(request.metricsOffset, view.metrics);
    if (
      !this.issueIndexComplete &&
      request.rule === null &&
      request.severity === null &&
      view.offset === this.findingOffset
    ) {
      this.issues = addIssuePage(this.issues, view.findings);
      this.findingOffset += view.findings.length;
      this.issueIndexComplete = this.findingOffset >= view.findingsCount;
    }
    const metricOffset = this.missingMetricWindow();
    if (this.issueIndexComplete && metricOffset === undefined) return;
    try {
      // Same serialized request lifetime, no new worker and no draft update.
      // Problems paging/filter state is never replaced by this projection.
      const page = await this.api.readQuality({
        ...request,
        run: false,
        restart: false,
        draft: null,
        selected: null,
        rule: null,
        severity: null,
        offset: this.findingOffset,
        limit: this.issueIndexComplete ? 0 : 200,
        metricsOffset: metricOffset ?? 0,
        metricsLimit: metricOffset === undefined ? 0 : 200,
      });
      if (
        !this.alive ||
        !this.input?.enabled ||
        this.paused ||
        version !== this.version ||
        identity !== this.reportKey ||
        page.phase !== 'ready'
      )
        return;
      if (identity !== this.reportIdentity(page)) {
        this.clearProjection();
        return;
      }
      if (!this.issueIndexComplete) {
        if (
          page.offset !== this.findingOffset ||
          (page.findings.length === 0 &&
            this.findingOffset < page.findingsCount)
        )
          throw new Error('Issue page did not advance; check again to retry.');
        this.issues = addIssuePage(this.issues, page.findings);
        this.findingOffset += page.findings.length;
        this.issueIndexComplete = this.findingOffset >= page.findingsCount;
      }
      if (metricOffset !== undefined)
        this.keepMetrics(metricOffset, page.metrics);
      this.projectionError = '';
    } catch (error) {
      if (
        this.alive &&
        this.input?.enabled &&
        !this.paused &&
        version === this.version &&
        identity === this.reportKey
      )
        this.projectionError = String(error);
    }
  }
  private clearTimer() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
  private schedule(delay: number) {
    this.clearTimer();
    if (this.alive && this.input?.enabled && !this.pending)
      this.timer = setTimeout(() => void this.poll(), delay);
  }
  async poll() {
    this.clearTimer();
    const input = this.input;
    if (!this.alive || !input?.enabled || this.pending) return;
    const sequence = this.sequence;
    const version = this.version;
    const draftSignature = JSON.stringify(input.draft);
    const request: QualityRequest = {
      session: input.session,
      generation: input.generation,
      revision: input.revision,
      run: !this.paused,
      restart: this.restartPending,
      offset: this.offset,
      limit: 100,
      metricsOffset: input.metricsOffset,
      metricsLimit: Math.min(200, input.metricsLimit),
      selected: input.selected,
      rule: this.rule,
      severity: this.severity,
      draft:
        draftSignature === this.draftSignature
          ? null
          : input.draft
            ? { kind: 'update', value: input.draft }
            : { kind: 'clear' },
    };
    this.restartPending = false;
    this.pending = true;
    try {
      const view = await this.api.readQuality(request);
      this.id = view.id;
      if (!this.alive || !this.input?.enabled || this.paused) {
        await this.api.cancelQuality(view.id).catch(() => {});
        if (!this.alive || !this.input?.enabled) return;
      }
      // A retired document/context cannot acknowledge the current draft. Within
      // the same context, Rust accepted the sent draft even if typing moved on.
      if (version !== this.version) return;
      if (request.draft) this.draftSignature = draftSignature;
      if (this.epoch !== null && this.epoch !== view.key.epoch)
        this.draftSignature = undefined;
      this.epoch = view.key.epoch;
      if (sequence !== this.sequence) return;
      if (
        view.key.generation !== input.generation ||
        view.key.revision !== input.revision
      )
        return;
      if (this.reportKey && this.reportKey !== this.reportIdentity(view))
        this.clearProjection();
      this.view = view;
      this.error = '';
      this.stale = false;
      await this.project(view, request, version);
    } catch (error) {
      if (this.alive && sequence === this.sequence) {
        this.error = String(error);
        this.view = null;
        this.clearProjection();
      }
    } finally {
      this.pending = false;
      this.schedule(
        sequence !== this.sequence ||
          (this.view?.phase === 'ready' && this.projectionPending)
          ? 0
          : this.view?.phase === 'ready' && !this.view.draftChecking
            ? 800
            : 180,
      );
    }
  }
  restart() {
    this.paused = false;
    this.restartPending = true;
    this.view = null;
    this.clearProjection();
    this.error = '';
    this.stale = true;
    ++this.sequence;
    this.schedule(0);
  }
  pause() {
    this.paused = true;
    ++this.sequence;
    if (this.id !== null)
      void this.api.cancelQuality(this.id).catch((e) => {
        if (this.alive) this.error = String(e);
      });
  }
  dispose() {
    this.alive = false;
    ++this.sequence;
    this.clearTimer();
    if (this.id !== null) void this.api.cancelQuality(this.id).catch(() => {});
  }
}
