import type { CueNavigation } from '../cues/cue-navigation.svelte';
import type { CueMenu } from '../cues/cue-menu.svelte';
import type { Workspace } from '../workspace/workspace.svelte';
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
import {
  planCueOperation,
  type CueOperationPlan,
} from '../cues/cue-operations';
import type { Cue, DocumentAction } from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
import type { MediaSession } from '../media/media-session.svelte';
import type { TimelineView } from '../timeline/timeline-view.svelte';

interface GestureEffects {
  cuts: () => number[];
  settings: () => TimingSettings;
  operationContext: () => Parameters<typeof planCueOperation>[1];
  command: (action: DocumentAction, plan: CueOperationPlan) => Promise<boolean>;
}

// Activity describes an in-progress commit or boundary preview. Workspace owns busy.
export class TimingGesture {
  // Connected once after navigation/menu construction, before effects or input.
  private navigation!: Pick<
    CueNavigation,
    'suspendFollowing' | 'revealSelection' | 'focusWaveform'
  >;
  private menu!: Pick<CueMenu, 'context'>;
  connect(navigation: typeof this.navigation, menu: typeof this.menu) {
    this.navigation = navigation;
    this.menu = menu;
  }
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
    private workspace: Workspace,
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
    this.navigation.suspendFollowing();
    this.pressedAt = performance.now();
    this.held = beginHeldTiming(this.subtitles.snapshot, this.media.status);
    this.workspace.notice = this.held
      ? 'Tap Space to pause; hold at least 400 ms to create a cue. Escape cancels.'
      : 'Start playback with ⌘/Ctrl+Alt+Enter, then hold Space to mark a cue.';
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
      this.workspace.busy ||
      this.workspace.confirming ||
      this.menu.context
    )
      return;
    const result = resolveTimingRelease(
      held,
      this.subtitles.snapshot,
      this.media.status,
      performance.now() - this.pressedAt,
    );
    if (!result) {
      this.workspace.notice =
        'Timing cancelled: playback changed or the interval was empty.';
      return;
    }
    if (result.kind === 'pause') {
      await this.workspace.run('play');
      return;
    }
    const marked = result.range;
    this.active = true;
    this.workspace.error = '';
    try {
      await this.subtitles.apply();
      if (!this.alive) return;
      const plan = planCueOperation('createCue', {
        ...this.effects.operationContext(),
        range: marked,
        draftDirty: false,
      });
      this.navigation.suspendFollowing();
      await this.effects.command(plan.action, plan);
      if (!this.alive) return;
      this.timeline.range = marked;
      await this.navigation.revealSelection();
      if (!this.alive) return;
      this.navigation.focusWaveform();
      this.workspace.notice =
        'Cue created · hold Space for the next cue; Enter edits text.';
    } catch (e) {
      if (this.alive) this.workspace.error = String(e);
    } finally {
      if (this.alive) this.active = false;
    }
  }
  beginBoundary(): boolean {
    this.cancelHeld();
    if (
      !this.alive ||
      !this.workspace.canAct ||
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
      this.navigation.suspendFollowing();
      this.workspace.error = '';
      this.workspace.notice =
        'Timing preview · release to apply, Escape to cancel';
      return true;
    } catch (e) {
      if (this.alive) this.workspace.error = String(e);
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
      this.workspace.notice = commit ? 'Timing applied' : 'Timing cancelled';
    } catch (e) {
      if (this.alive) this.workspace.error = String(e);
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
