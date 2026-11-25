/* eslint-disable svelte/prefer-svelte-reactivity -- Collections are replaced as immutable projections; the private metric cache is not reactive state. */
import { cueIssueMap, type IssueSeverity } from './cue-issues';
import {
  readQuality,
  cancelQuality,
  type QualityRequest,
  type QualityView,
  type QualityDraft,
} from '../client';

import type { WorkspaceActivity } from '../workspace/workspace-activity.svelte';

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
  // Keep the configure debounce separate from the visual hold on an IPC read.
  refreshStarted = $state(false);
  listStale = $state(true);
  displayedList = $state.raw<QualityView | null>(null);
  // During a same-context revision, the table can paint the last complete index
  // until the compact index arrives. Commands only consume the current index.
  private retainedIndex = $state.raw<{
    issues: QualitySession['issues'];
    metrics: QualitySession['metrics'];
  } | null>(null);
  get displayedIssues() {
    return this.retainedIndex?.issues ?? this.issues;
  }
  get displayedMetrics() {
    return this.retainedIndex?.metrics ?? this.metrics;
  }
  get retainingIndex() {
    return this.retainedIndex !== null;
  }
  lastDraft = $state.raw<QualityView['draft']>(null);
  private clearDisplay() {
    this.retainedIndex = null;
    this.displayedList = null;
    this.lastDraft = null;
  }
  private listQuery = '';
  paused = $state(false);
  rule = $state<QualityRequest['rule']>(null);
  severity = $state<QualityRequest['severity']>(null);
  offset = $state(0);
  selectedOffset = $state(0);
  issues = $state.raw<ReadonlyMap<string, IssueSeverity>>(new Map());
  metrics = $state.raw<ReadonlyMap<string, QualityView['metrics'][number]>>(
    new Map(),
  );
  issueIndexComplete = $state(false);
  projectionError = $state('');
  private reportKey = '';
  private indexedReport = 0;
  private afterWorker: number | null = null;
  private due = 0;
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
  private userKey = '';
  private nextUserRead = false;
  constructor(
    private api = client,
    private activity?: WorkspaceActivity,
  ) {}

  userSelectedCue() {
    this.nextUserRead = true;
  }
  configure(input: QualityInput) {
    if (!this.alive) return;
    const signature = JSON.stringify([
      input,
      this.rule,
      this.severity,
      this.offset,
      this.selectedOffset,
    ]);
    if (signature === this.signature) return;
    const version = `${input.session}/${input.generation}/${input.revision}/${input.context}`;
    const replaced =
      !input.enabled ||
      this.input?.session !== input.session ||
      this.input.generation !== input.generation ||
      this.input.context !== input.context;
    if (replaced) this.clearDisplay();
    else if (version !== this.version && this.issueIndexComplete)
      this.retainedIndex = { issues: this.issues, metrics: this.metrics };
    if (input.selected !== this.input?.selected || !input.draft)
      this.lastDraft = null;
    if (version !== this.version) {
      this.version = version;
      this.view = null;
      this.clearProjection();
      this.draftSignature = undefined;
      this.epoch = null;
      this.offset = 0;
    }
    if (input.selected !== this.input?.selected) this.selectedOffset = 0;
    const listQuery = JSON.stringify([
      version,
      this.rule,
      this.severity,
      this.offset,
    ]);
    if (listQuery !== this.listQuery || !input.enabled) this.listStale = true;
    this.listQuery = listQuery;
    const userKey = JSON.stringify([
      version,
      this.rule,
      this.severity,
      this.offset,
      this.selectedOffset,
    ]);
    if (userKey !== this.userKey) {
      this.nextUserRead = input.enabled;
      this.userKey = userKey;
    }
    const committed =
      !!this.input &&
      this.input.generation === input.generation &&
      this.input.revision !== input.revision;
    this.afterWorker = null;
    this.due = performance.now() + (committed ? 0 : 180);
    this.refreshStarted = false;
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
    } else this.schedule(Math.max(0, this.due - performance.now()));
  }
  private clearProjection() {
    this.reportKey = '';
    this.indexedReport = 0;
    this.issues = new Map();
    this.metrics = new Map();
    this.metricWindows.clear();
    this.issueIndexComplete = false;
    this.projectionError = '';
  }
  private reportIdentity(view: QualityView) {
    return JSON.stringify([view.reportId ?? view.id, view.key]);
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
    if (view.issueIndex) {
      this.issues = cueIssueMap(view.issueIndex);
      this.indexedReport = view.reportId ?? view.id;
      this.issueIndexComplete = true;
      // Keep a single User Timing entry for commit-to-index measurements.
      performance.clearMarks('quality-index-complete');
      performance.mark('quality-index-complete', {
        detail: {
          generation: request.generation,
          revision: request.revision,
          cues: this.issues.size,
          findings: view.findingsCount,
        },
      });
    }
    const sequence = this.sequence;
    try {
      if (!this.issueIndexComplete)
        throw new Error('Ready report omitted its issue index');
      let offset = this.missingMetricWindow();
      while (offset !== undefined) {
        const page = await this.api.readQuality({
          ...request,
          run: false,
          restart: false,
          draft: null,
          selected: null,
          indexSince: null,
          afterWorker: null,
          rule: null,
          severity: null,
          offset: 0,
          limit: 0,
          metricsOffset: offset,
          metricsLimit: 200,
        });
        if (
          !this.alive ||
          !this.input?.enabled ||
          this.paused ||
          sequence !== this.sequence ||
          version !== this.version ||
          page.phase !== 'ready' ||
          identity !== this.reportIdentity(page)
        )
          return;
        this.keepMetrics(offset, page.metrics);
        offset = this.missingMetricWindow();
      }
      this.projectionError = '';
    } catch (error) {
      if (
        this.alive &&
        this.input?.enabled &&
        !this.paused &&
        sequence === this.sequence &&
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
      afterWorker: this.afterWorker,
      indexSince: input.indexIssues ? this.indexedReport : null,
      restart: this.restartPending,
      offset: this.offset,
      limit: 100,
      metricsOffset: input.metricsOffset,
      metricsLimit: Math.min(200, input.metricsLimit),
      selected: input.selected,
      selectedOffset: this.selectedOffset,
      rule: this.rule,
      severity: this.severity,
      draft:
        draftSignature === this.draftSignature
          ? null
          : input.draft
            ? { kind: 'update', value: input.draft }
            : { kind: 'clear' },
    };
    this.afterWorker = null;
    this.restartPending = false;
    this.pending = true;
    this.refreshStarted = true;
    try {
      const userRead = this.nextUserRead;
      this.nextUserRead = false;
      const read = this.api.readQuality(request);
      const view = await (userRead && this.activity
        ? this.activity.track(read)
        : read);
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
      if (this.displayedList && this.displayedList.key.epoch !== view.key.epoch)
        this.clearDisplay();
      if (this.reportKey && this.reportKey !== this.reportIdentity(view))
        this.clearProjection();
      this.listStale = view.phase !== 'ready';
      if (!this.listStale) this.displayedList = view;
      if (
        view.draft &&
        input.draft &&
        view.draft.data.cueId === String(input.selected)
      )
        this.lastDraft = view.draft;
      if (
        !this.paused &&
        (view.phase === 'checking' ||
          view.phase === 'cancelling' ||
          (view.phase === 'ready' && view.draftChecking))
      )
        this.afterWorker = view.id;
      this.view = view;
      this.error = '';
      this.stale = false;
      await this.project(view, request, version);
      if (
        version === this.version &&
        this.issueIndexComplete &&
        this.missingMetricWindow() === undefined
      )
        this.retainedIndex = null;
    } catch (error) {
      if (this.alive && sequence === this.sequence) {
        this.clearDisplay();
        this.error = String(error);
        this.listStale = true;
        this.stale = true;
        this.view = null;
        this.clearProjection();
      }
    } finally {
      this.pending = false;
      this.schedule(
        sequence !== this.sequence
          ? Math.max(0, this.due - performance.now())
          : this.afterWorker !== null ||
              (this.view?.phase === 'ready' && this.projectionPending)
            ? 0
            : this.view?.phase === 'ready' && !this.view.draftChecking
              ? 800
              : 180,
      );
    }
  }
  restart() {
    this.afterWorker = null;
    this.due = 0;
    this.nextUserRead = true;
    this.paused = false;
    this.restartPending = true;
    this.listStale = true;
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
    this.clearDisplay();
    this.clearProjection();
    this.view = null;
    ++this.sequence;
    this.clearTimer();
    if (this.id !== null) void this.api.cancelQuality(this.id).catch(() => {});
  }
}
