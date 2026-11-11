import {
  beginHeldTiming,
  advanceHeldTiming,
  resolveTimingRelease,
  type HeldTiming,
} from './hold-timing';
import {
  beginTimingEdit,
  previewBoundary,
  timingAction,
  type TimingEdit,
  type CueBoundary,
} from './cue-timing';
import { snapBoundary, type TimingSettings } from './magnetic-timing';
import { planCueOperation, type CueOperationPlan } from './cue-operations';
import type { Cue, DocumentAction } from './client';
import type { DocumentSession } from './document-session.svelte';
import type { MediaSession } from './media-session.svelte';
import type { TimelineView } from './timeline-view.svelte';

interface GestureEffects {
  busy: () => boolean;
  confirming: () => boolean;
  ready: () => boolean;
  menu: () => unknown;
  cuts: () => number[];
  settings: () => TimingSettings;
  operationContext: () => Parameters<typeof planCueOperation>[1];
  command: (action: DocumentAction, plan: CueOperationPlan) => Promise<boolean>;
  run: (action: 'play') => Promise<void>;
  suspendFollowing: () => void;
  revealSelection: () => Promise<void>;
  focusWaveform: () => void;
  error: (message: string) => void;
  notice: (message: string) => void;
}

// Activity describes an in-progress commit or boundary preview. App owns busy.
export class TimingGesture {
  mode = $state(false);
  held = $state<HeldTiming | null>(null);
  edit = $state<TimingEdit | null>(null);
  active = $state(false);
  snapHint = $state('');
  private alive = true;
  private pressedAt = 0;
  private magnets: {
    cues: Cue[];
    cuts: number[];
    settings: TimingSettings;
  } | null = null;
  private rangeBefore: [number, number] | null = null;
  constructor(
    private subtitles: DocumentSession,
    private media: MediaSession,
    private timeline: TimelineView,
    private effects: GestureEffects,
  ) {
    $effect(() => {
      if (!this.alive || !this.held) return;
      const next =
        this.mode && !this.media.pending
          ? advanceHeldTiming(
              this.held,
              this.subtitles.snapshot,
              this.media.status,
            )
          : null;
      if (!next || next.last !== this.held.last) this.held = next;
    });
  }
  beginHeld() {
    if (!this.alive) return;
    this.effects.suspendFollowing();
    this.pressedAt = performance.now();
    this.held = beginHeldTiming(this.subtitles.snapshot, this.media.status);
    this.effects.notice(
      this.held
        ? 'Tap Space to pause; hold at least 400 ms to create a cue. Escape cancels.'
        : 'Start playback with ⌘/Ctrl+Alt+Enter, then hold Space to mark a cue.',
    );
  }
  cancelHeld() {
    this.held = null;
  }
  async releaseHeld() {
    const held = this.held;
    this.held = null;
    if (
      !this.alive ||
      !held ||
      this.effects.busy() ||
      this.effects.confirming() ||
      this.effects.menu()
    )
      return;
    const result = resolveTimingRelease(
      held,
      this.subtitles.snapshot,
      this.media.status,
      performance.now() - this.pressedAt,
    );
    if (!result) {
      this.effects.notice(
        'Timing cancelled: playback changed or the interval was empty.',
      );
      return;
    }
    if (result.kind === 'pause') {
      await this.effects.run('play');
      return;
    }
    const marked = result.range;
    this.active = true;
    this.effects.error('');
    try {
      await this.subtitles.apply();
      if (!this.alive) return;
      const plan = planCueOperation('createCue', {
        ...this.effects.operationContext(),
        range: marked,
        draftDirty: false,
      });
      this.effects.suspendFollowing();
      await this.effects.command(plan.action, plan);
      if (!this.alive) return;
      this.timeline.range = marked;
      await this.effects.revealSelection();
      if (!this.alive) return;
      this.effects.focusWaveform();
      this.effects.notice(
        'Cue created · hold Space for the next cue; Enter edits text.',
      );
    } catch (e) {
      if (this.alive) this.effects.error(String(e));
    } finally {
      if (this.alive) this.active = false;
    }
  }
  beginBoundary(): boolean {
    this.cancelHeld();
    if (
      !this.alive ||
      this.effects.busy() ||
      this.effects.confirming() ||
      !this.effects.ready() ||
      !this.media.status.loaded ||
      this.media.pending
    )
      return false;
    try {
      this.edit = beginTimingEdit(
        this.subtitles.snapshot,
        this.subtitles.selected,
        this.media.status.session,
        this.subtitles.dirty,
      );
      this.rangeBefore = this.timeline.range;
      this.magnets = {
        cues: this.subtitles.snapshot.cues,
        cuts: [...this.effects.cuts()],
        settings: { ...this.effects.settings() },
      };
      this.active = true;
      this.effects.suspendFollowing();
      this.effects.error('');
      this.effects.notice(
        'Timing preview · release to apply, Escape to cancel',
      );
      return true;
    } catch (e) {
      if (this.alive) this.effects.error(String(e));
      return false;
    }
  }
  preview(
    boundary: CueBoundary,
    milliseconds: number,
    pointer?: { bypass: boolean; millisecondsPerPixel: number },
  ) {
    if (this.alive && this.edit && this.magnets) {
      const result = snapBoundary(
        milliseconds,
        boundary,
        boundary === 'start'
          ? this.edit.preview.endMs
          : this.edit.preview.startMs,
        this.edit.original.id,
        this.magnets.cues,
        this.magnets.cuts,
        this.magnets.settings,
        (pointer?.millisecondsPerPixel ?? 0) *
          this.magnets.settings.snapDistancePx,
        Math.floor(this.timeline.duration * 1000),
        !pointer || pointer.bypass,
      );
      this.snapHint = result.target ? `Snapped to ${result.target}` : '';
      this.edit = previewBoundary(this.edit, boundary, result.milliseconds);
      this.timeline.selectRange([
        this.edit.preview.startMs / 1000,
        this.edit.preview.endMs / 1000,
      ]);
    }
  }
  async finish(commit: boolean) {
    const edit = this.edit;
    if (!this.alive || !edit) return;
    let accepted = false;
    try {
      const action = commit
        ? timingAction(
            edit,
            this.subtitles.snapshot,
            this.media.status.session,
            this.subtitles.selected,
          )
        : null;
      if (action) {
        // Send the gesture's captured version, never substitute the latest one.
        await this.subtitles.command(action, {
          version: edit,
          preferred: edit.original.id,
        });
      }
      if (!this.alive) return;
      accepted = commit;
      this.effects.notice(commit ? 'Timing applied' : 'Timing cancelled');
    } catch (e) {
      if (this.alive) this.effects.error(String(e));
    } finally {
      if (this.alive) {
        const finalCue = accepted ? edit.preview : edit.original;
        this.timeline.range = accepted
          ? [finalCue.startMs / 1000, finalCue.endMs / 1000]
          : this.rangeBefore;
        this.media.loop.update(
          edit.mediaSession,
          this.timeline.range ?? [
            finalCue.startMs / 1000,
            finalCue.endMs / 1000,
          ],
        );
        await this.media.loop.flush();
        if (this.alive) {
          this.edit = null;
          this.magnets = null;
          this.snapHint = '';
          this.active = false;
        }
      }
    }
  }

  dispose() {
    this.alive = false;
    this.held = null;
    this.edit = null;
    this.magnets = null;
    this.active = false;
  }
}
