import { cueGroupActions, type CueGroupActions } from '../client';
import type { CueNavigation } from './cue-navigation.svelte';
import { HideVideo, type HideVideoEffects } from '../dialogs/hide-video.svelte';
import type { Workspace } from '../workspace/workspace.svelte';
import type { DocumentSession } from '../document/document-session.svelte';
import type { MediaSession } from '../media/media-session.svelte';
import type { TimelineView } from '../timeline/timeline-view.svelte';
import type { TimingGesture } from '../timing/timing-gesture.svelte';
import type { CueEditing } from './cue-editing.svelte';
import { commitCueGroup, commitSplitInHalf } from './cue-actions';
import {
  planCueOperation,
  orderedGroupIds,
  type CueOperation,
} from './cue-operations';

interface MenuContext {
  x: number;
  y: number;
  cueId: number | null;
  tableIds: number[] | null;
  group: CueGroupActions | null;
  midpointReason: string | null;
  range: [number, number] | null;
  generation: number;
  revision: number;
  session: number;
}
interface Effects extends HideVideoEffects {
  splitMidpoint: CueEditing['splitMidpoint'];
  captureFocus: () => () => void;
  operationContext: () => Parameters<typeof planCueOperation>[1];
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
    private navigation: CueNavigation,
    private effects: Effects,
  ) {
    this.video = new HideVideo(effects);
  }
  get active() {
    return this.video.opening || this.committing;
  }
  get items(): { id: string; label: string; reason?: string | null }[] {
    const menu = this.context;
    if (!menu) return [];
    const ids = menu.tableIds;
    const group = menu.group;
    if (ids && group) {
      return (
        [
          ['mergeRun', 'Merge'],
          ['mergeAsDialog', 'Merge as dialog'],
          ['deleteMany', `Delete ${ids.length} cues`],
        ] as const
      ).map(([id, label]) => ({
        id,
        label,
        reason: group[id],
      }));
    }
    const items: { id: string; label: string; reason?: string | null }[] = [];
    if (menu.range)
      items.push({ id: 'create', label: 'Create cue from selection' });
    if (menu.cueId !== null) {
      items.push({ id: 'edit', label: 'Edit cue text' });
      items.push({
        id: 'splitInHalf',
        label: 'Split in half',
        reason: menu.midpointReason,
      });
      if (ids) {
        for (const [id, label] of [
          ['mergePrevious', 'Merge with previous'],
          ['mergeNext', 'Merge with next'],
        ] as const) {
          let reason: string | null = null;
          try {
            this.singlePlan(id, menu.cueId);
          } catch (error) {
            reason = error instanceof Error ? error.message : String(error);
          }
          items.push({ id, label, reason });
        }
      }
      items.push({ id: 'delete', label: 'Delete cue' });
    }
    return items;
  }
  private singlePlan(action: CueOperation, id: number) {
    return planCueOperation(action, {
      ...this.effects.operationContext(),
      selected: id,
      draftDirty: false,
    });
  }
  async open(
    x: number,
    y: number,
    cueId: number | null,
    inRange: boolean,
    row?: HTMLButtonElement,
  ) {
    if (
      this.committing ||
      !this.workspace.canAct ||
      (cueId === null && !inRange) ||
      this.video.disposed
    )
      return;
    this.gestures.cancelHeld();
    const doc = this.subtitles.snapshot;
    const session = this.media.status.session;
    const tableIds =
      row && cueId !== null
        ? this.subtitles.selectedIds.has(cueId)
          ? [...this.subtitles.selectedIds]
          : [cueId]
        : null;
    const returnFocus = row
      ? () => row.focus({ preventScroll: true })
      : this.effects.captureFocus();
    let attemptedHide = false;
    try {
      if (tableIds && cueId !== null) {
        this.committing = true;
        if (!(await this.subtitles.apply()) || this.video.disposed) return;
        if (
          this.subtitles.snapshot.generation !== doc.generation ||
          this.media.status.session !== session
        )
          throw new Error(
            'The document or media changed; open the menu again.',
          );
        this.navigation.suspendFollowing();
        if (!this.subtitles.selectedIds.has(cueId))
          this.subtitles.select(cueId);
      }
      const candidate: MenuContext = {
        group: null,
        midpointReason: null,
        x,
        y,
        cueId,
        tableIds,
        range: inRange && this.timeline.range ? [...this.timeline.range] : null,
        generation: doc.generation,
        revision: this.subtitles.snapshot.revision,
        session,
      };
      const verify = () => {
        const current = this.subtitles.snapshot;
        if (
          candidate.generation !== current.generation ||
          candidate.revision !== current.revision ||
          session !== this.media.status.session
        )
          throw new Error(
            'The document or media changed; open the menu again.',
          );
      };
      if (tableIds && tableIds.length > 1) {
        candidate.group = await cueGroupActions(
          candidate,
          orderedGroupIds(tableIds, this.subtitles.snapshot.cues),
        );
        if (this.video.disposed) return;
        verify();
      } else if (cueId !== null) {
        try {
          await this.effects.splitMidpoint(candidate, cueId);
        } catch (error) {
          candidate.midpointReason = String(error);
        }
        if (this.video.disposed) return;
        verify();
      }
      this.returnFocus = returnFocus;
      attemptedHide = true;
      await this.video.open(
        () => {
          verify();
          this.context = candidate;
        },
        { replace: true },
      );
    } catch (error) {
      if (!this.video.disposed) {
        this.workspace.error = String(error);
        await this.video.close({
          hide: () => {
            this.context = null;
            this.returnFocus = null;
          },
          restoreWhenIdle: attemptedHide,
          failed: (restoreError) => {
            this.workspace.error = String(error) + ' · ' + String(restoreError);
          },
        });
      }
    } finally {
      this.committing = false;
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
    const item = this.items.find((item) => item.id === action);
    if (!item || item.reason) return;
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
      if (!(await this.subtitles.apply()) || this.video.disposed) return;
      if (
        context.generation !== this.subtitles.snapshot.generation ||
        context.session !== this.media.status.session
      )
        throw new Error('The document or media changed; open the menu again.');
      this.navigation.suspendFollowing();
      if (action === 'create' && context.range) {
        const plan = planCueOperation('createCue', {
          ...this.effects.operationContext(),
          range: context.range,
          draftDirty: false,
        });
        await this.subtitles.command(plan.action, { plan });
        if (this.video.disposed) return;
        await this.navigation.revealSelection();
        if (this.video.disposed) return;
        await this.navigation.focusText();
      } else if (
        context.tableIds &&
        (action === 'mergeRun' ||
          action === 'mergeAsDialog' ||
          action === 'deleteMany')
      ) {
        if (
          (await commitCueGroup(action, context.tableIds, this.subtitles)) &&
          !this.video.disposed
        )
          await this.navigation.revealSelection();
      } else if (context.cueId !== null) {
        if (
          !this.subtitles.snapshot.cues.some((cue) => cue.id === context.cueId)
        )
          throw new Error('This cue no longer exists.');
        if (action === 'splitInHalf') {
          if (
            (await commitSplitInHalf(
              this.subtitles,
              context.cueId,
              this.effects.splitMidpoint,
            )) &&
            !this.video.disposed
          )
            await this.navigation.revealSelection();
        } else if (
          action === 'delete' ||
          action === 'mergePrevious' ||
          action === 'mergeNext'
        ) {
          const plan = this.singlePlan(
            action === 'delete' ? 'deleteCue' : action,
            context.cueId,
          );
          await this.subtitles.command(plan.action, { plan });
          if (this.video.disposed) return;
          if (action === 'delete') this.navigation.focusWaveform();
          else await this.navigation.revealSelection();
        } else {
          this.subtitles.select(context.cueId);
          await this.navigation.revealSelection();
          if (this.video.disposed) return;
          await this.navigation.focusText();
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
