import type { DocumentSession } from '../document/document-session.svelte';
import type { MediaSession } from '../media/media-session.svelte';
import type { TimelineView } from '../timeline/timeline-view.svelte';
import {
  cueOperationLabels,
  planCueOperation,
  type CueOperation,
} from './cue-operations';
export const AUTOCOMMIT_DELAY_MS = 700;

export class CueEditing {
  textSession = $state<string | null>(null);
  composing = $state(false);
  escaping = $state(false);
  private timer: ReturnType<typeof setTimeout> | null = null;
  private alive = true;
  private blurPending = false;
  private observed = '';
  private lastInputAt = 0;
  private allowed = false;
  constructor(
    private subtitles: DocumentSession,
    private media: Pick<MediaSession, 'status' | 'pending'>,
    private timeline: Pick<TimelineView, 'range'>,
    private effects: {
      canCommit: () => boolean;
      leaveEditor: () => void;
    },
  ) {
    subtitles.connectCommitSession(() => this.textSession);
    $effect(() => {
      const signature = JSON.stringify([
        subtitles.snapshot.generation,
        subtitles.selected,
        subtitles.text,
        subtitles.start,
        subtitles.end,
        subtitles.style,
      ]);
      if (signature !== this.observed) {
        this.observed = signature;
        this.lastInputAt = performance.now();
        if (subtitles.stateConfirmed) subtitles.editError = '';
      }
      const allowed = effects.canCommit();
      if (allowed && !this.allowed) this.lastInputAt = performance.now();
      this.allowed = allowed;
      this.clearTimer();
      if (
        !this.alive ||
        this.composing ||
        this.escaping ||
        !allowed ||
        !subtitles.cue ||
        !subtitles.dirty ||
        subtitles.committing ||
        subtitles.editError ||
        !subtitles.stateConfirmed
      )
        return;
      this.timer = setTimeout(
        () => {
          this.timer = null;
          if (this.alive)
            void subtitles.commitAutomatically(this.textSession).catch(() => {
              // DocumentSession retains the fields and publishes the inline error.
            });
        },
        Math.max(
          0,
          AUTOCOMMIT_DELAY_MS - (performance.now() - this.lastInputAt),
        ),
      );
    });
  }
  private clearTimer() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
  focusText() {
    if (this.alive && this.textSession === null)
      this.textSession = crypto.randomUUID();
  }
  composition(active: boolean) {
    if (!this.alive) return;
    this.composing = active;
    if (active) this.clearTimer();
    else {
      this.lastInputAt = performance.now();
      if (this.blurPending) {
        this.blurPending = false;
        this.blur();
      }
    }
  }
  blur() {
    if (!this.alive) return;
    if (this.composing) {
      this.blurPending = true;
      return;
    }
    // Modal/decision focus transfer already owns draft protection. Do not
    // mutate underneath a newly mounted review or a Save/Discard decision.
    if (this.effects.canCommit()) void this.flush().catch(() => {});
    this.textSession = null;
  }
  async flush(): Promise<boolean> {
    this.clearTimer();
    if (!this.alive || this.composing) return false;
    return this.subtitles.apply(this.textSession);
  }
  async escape() {
    this.clearTimer();
    this.textSession = null;
    this.escaping = true;
    try {
      await this.subtitles.waitForCommit().catch(() => false);
      if (
        !this.alive ||
        !this.subtitles.stateConfirmed ||
        (!this.subtitles.cue && this.subtitles.dirty)
      )
        return;
      this.subtitles.revert();
      this.subtitles.editError = '';
      this.effects.leaveEditor();
    } finally {
      if (this.alive) this.escaping = false;
    }
  }
  dispose() {
    this.alive = false;
    this.clearTimer();
    this.textSession = null;
  }
  context = $derived.by(() => {
    const doc = this.subtitles.snapshot;
    const selected = this.subtitles.selected;
    const draftDirty = this.subtitles.dirty;
    const status = this.media.status;
    return {
      timeStepMs: doc.format?.timeStepMs ?? 1,
      cues: doc.cues,
      selected,
      loaded: doc.loaded,
      draftDirty,
      positionSeconds:
        status.loaded && !status.seeking && !this.media.pending
          ? status.positionSeconds
          : null,
      range: this.timeline.range,
    };
  });
  operations = $derived.by(() => {
    const context = this.context;
    return (Object.entries(cueOperationLabels) as [CueOperation, string][]).map(
      ([operation, label]) => {
        try {
          return {
            operation,
            label,
            plan: planCueOperation(operation, {
              ...context,
              draftDirty: false,
            }),
            reason: '',
          };
        } catch (e) {
          return {
            operation,
            label,
            plan: null,
            reason: String(e).replace(/^Error: /, ''),
          };
        }
      },
    );
  });
}
