import { HideVideo, type HideVideoEffects } from './hide-video.svelte';
import type { Workspace } from './workspace.svelte';
import type { DocumentSession } from './document-session.svelte';
import type { MediaSession } from './media-session.svelte';
import type { TimelineView } from './timeline-view.svelte';
import type { TimingGesture } from './timing-gesture.svelte';
import { planCueOperation } from './cue-operations';

interface MenuContext {
  x: number;
  y: number;
  cueId: number | null;
  range: [number, number] | null;
  generation: number;
  revision: number;
  session: number;
}
interface Effects extends HideVideoEffects {
  captureFocus: () => () => void;
  operationContext: () => Parameters<typeof planCueOperation>[1];
  suspendFollowing: () => void;
  revealSelection: () => Promise<void>;
  focusText: () => Promise<void>;
  focusWaveform: () => void;
}

export class CueMenu {
  context = $state.raw<MenuContext | null>(null);
  private committing = $state(false);
  private video: HideVideo;
  private returnFocus: (() => void) | null = null;
  constructor(
    private workspace: Workspace,
    private subtitles: DocumentSession,
    private media: MediaSession,
    private timeline: TimelineView,
    private gestures: TimingGesture,
    private effects: Effects,
  ) {
    this.video = new HideVideo(effects);
  }
  get active() {
    return this.video.opening || this.committing;
  }
  get items() {
    const menu = this.context;
    return menu
      ? [
          ...(menu.range
            ? [{ id: 'create', label: 'Create cue from selection' }]
            : []),
          ...(menu.cueId !== null
            ? [
                { id: 'edit', label: 'Edit cue text' },
                { id: 'delete', label: 'Delete cue' },
              ]
            : []),
        ]
      : [];
  }
  async open(x: number, y: number, cueId: number | null, inRange: boolean) {
    if (
      !this.workspace.canAct ||
      (cueId === null && !inRange) ||
      this.video.disposed
    )
      return;
    this.gestures.cancelHeld();
    const doc = this.subtitles.snapshot;
    const candidate: MenuContext = {
      x,
      y,
      cueId,
      range: inRange && this.timeline.range ? [...this.timeline.range] : null,
      generation: doc.generation,
      revision: doc.revision,
      session: this.media.status.session,
    };
    this.returnFocus = this.effects.captureFocus();
    try {
      await this.video.open(
        () => {
          this.context = candidate;
        },
        { replace: true },
      );
    } catch (error) {
      if (!this.video.disposed) this.workspace.error = String(error);
    }
  }
  close(restoreFocus = true) {
    return this.video.close({
      hide: () => {
        this.context = null;
        // Menu cancellation returns focus immediately; modals wait for their
        // disabled launcher to render after restoration instead.
        if (restoreFocus) this.returnFocus?.();
        this.returnFocus = null;
      },
      failed: (error) => {
        this.workspace.error = String(error);
      },
    });
  }
  async choose(action: string) {
    const context = this.context;
    if (!context || this.workspace.busy || this.video.disposed) return;
    this.committing = true;
    this.workspace.error = '';
    try {
      await this.close(false);
      if (this.video.disposed) return;
      const doc = this.subtitles.snapshot;
      if (
        context.generation !== doc.generation ||
        context.revision !== doc.revision ||
        context.session !== this.media.status.session
      )
        throw new Error('The document or media changed; open the menu again.');
      await this.subtitles.apply();
      if (this.video.disposed) return;
      this.effects.suspendFollowing();
      if (action === 'create' && context.range) {
        const plan = planCueOperation('createCue', {
          ...this.effects.operationContext(),
          range: context.range,
          draftDirty: false,
        });
        await this.subtitles.command(plan.action, { plan });
        if (this.video.disposed) return;
        await this.effects.revealSelection();
        if (this.video.disposed) return;
        await this.effects.focusText();
      } else if (context.cueId !== null) {
        if (
          !this.subtitles.snapshot.cues.some((cue) => cue.id === context.cueId)
        )
          throw new Error('This cue no longer exists.');
        if (action === 'delete') {
          const plan = planCueOperation('deleteCue', {
            ...this.effects.operationContext(),
            selected: context.cueId,
            draftDirty: false,
          });
          await this.subtitles.command(plan.action, { plan });
          if (this.video.disposed) return;
          this.effects.focusWaveform();
        } else {
          this.subtitles.select(context.cueId);
          await this.effects.revealSelection();
          if (this.video.disposed) return;
          await this.effects.focusText();
        }
      }
    } catch (error) {
      if (!this.video.disposed) this.workspace.error = String(error);
    } finally {
      if (!this.video.disposed) this.committing = false;
    }
  }
  dispose() {
    this.video.dispose();
    this.context = null;
    this.returnFocus = null;
  }
}
