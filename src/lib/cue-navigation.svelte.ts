import { tick, untrack } from 'svelte';
import { clamp } from './timeline';
import { commandMedia } from './media-actions';
import {
  changeFollowing,
  followedSelection,
  revealCueRow,
  type CueFollowing,
} from './cue-following';
import type { SelectionMode } from './cue-selection';
import type { DocumentSnapshot } from './client';
import type { Workspace } from './workspace.svelte';
import type { DocumentSession } from './document-session.svelte';
import type { MediaSession } from './media-session.svelte';
import type { TimelineView } from './timeline-view.svelte';
import type { CueTableView } from './cue-table-view.svelte';
import type { TimingGesture } from './timing-gesture.svelte';
import type { CueMenu } from './cue-menu.svelte';
interface Effects {
  readList: () => { scrollTop: number; height: number } | null;
  scrollList: (top: number) => number;
  focusCue: (id: number) => void;
  focusText: () => void;
  focusWaveform: () => void;
}
export class CueNavigation {
  following = $state<CueFollowing>('following');
  editorFocused = $state(false);
  textFocused = $state(false);
  pendingProblem = $state<string | null>(null);
  private pendingProblemVersion = '';
  private lastRevealedCue = '';
  private alive = true;
  private cueMenu: Pick<CueMenu, 'context'> | null = null;
  connectMenu(menu: Pick<CueMenu, 'context'>) {
    this.cueMenu = menu;
  }
  constructor(
    private workspace: Workspace,
    private subtitles: DocumentSession,
    private media: MediaSession,
    private timeline: TimelineView,
    private tableView: CueTableView,
    private gestures: TimingGesture,
    private effects: Effects,
  ) {
    $effect(() => {
      if (!this.alive) return;
      if (
        this.pendingProblem &&
        (this.pendingProblemVersion !==
          `${this.subtitles.snapshot.generation}/${this.subtitles.snapshot.revision}` ||
          !this.subtitles.snapshot.cues.some(
            (c) => String(c.id) === this.pendingProblem,
          ))
      )
        this.pendingProblem = null;
    });
    $effect(() => {
      if (!this.alive) return;
      // Includes timing buttons and other draft changes that do not focus an input.
      if (this.subtitles.dirty || this.editorFocused) this.suspendFollowing();
    });
    $effect(() => {
      if (!this.alive) return;
      const protectedEdit =
        this.subtitles.dirty ||
        this.editorFocused ||
        !this.workspace.canAct ||
        this.media.pending;
      const next = followedSelection(
        this.tableView.activeIds,
        this.subtitles.selected,
        this.following,
        protectedEdit,
      );
      if (this.following !== 'following') this.lastRevealedCue = '';
      if (
        protectedEdit ||
        this.following !== 'following' ||
        next === null ||
        !this.tableView.activeIds.includes(next)
      )
        return;
      const index = this.subtitles.snapshot.cues.findIndex(
        (item) => item.id === next,
      );
      const key = `${this.subtitles.snapshot.generation}:${next}:${this.tableView.height}`;
      untrack(() => {
        // Automatic selection never calls selectCue: that path commits and seeks.
        if (next !== this.subtitles.selected) {
          this.subtitles.select(next);
        }
        const viewport = this.effects.readList();
        if (viewport && key !== this.lastRevealedCue) {
          this.tableView.scrollTop = this.effects.scrollList(
            revealCueRow(index, viewport.scrollTop, this.tableView.height),
          );
          this.lastRevealedCue = key;
        }
      });
    });
  }
  focusEntered = (event: FocusEvent) => {
    this.textFocused =
      event.target instanceof HTMLInputElement ||
      event.target instanceof HTMLTextAreaElement;
  };
  focusWaveform = () => {
    this.effects.focusWaveform();
  };
  focusText = async () => {
    this.suspendFollowing();
    await tick();
    if (this.alive && this.subtitles.cue) this.effects.focusText();
  };
  selectCue = async (
    id: number,
    mode: SelectionMode = 'replace',
    expected?: DocumentSnapshot,
  ) => {
    // Rows use aria-disabled so transport commands do not drop keyboard focus.
    // Enforce the pending-operation guard here for both pointer and keyboard use.
    // Preserve row/continuation selection’s busy-only admission; problem and
    // select-all entry points additionally reject confirming.
    if (
      !this.alive ||
      this.workspace.busy ||
      this.cueMenu?.context ||
      (expected && this.subtitles.snapshot !== expected) ||
      !this.subtitles.snapshot.cues.some((c) => c.id === id)
    )
      return;
    this.suspendFollowing();
    this.gestures.cancelHeld();
    await this.workspace.task(async () => {
      await this.subtitles.apply();
      if (!this.alive || (expected && this.subtitles.snapshot !== expected))
        return;
      this.subtitles.select(id, mode);
      const selectedCue = this.subtitles.cue;
      if (selectedCue)
        this.timeline.selectRange([
          selectedCue.startMs / 1000,
          selectedCue.endMs / 1000,
        ]);
      await this.revealSelection();
      if (!this.alive || (expected && this.subtitles.snapshot !== expected))
        return;
      if (this.subtitles.cue && this.media.status.loaded)
        await commandMedia(
          { media: this.media, gestures: this.gestures },
          { kind: 'seek', seconds: this.subtitles.cue.startMs / 1000 },
        );
    });
  };
  cueSelectionKey = (e: KeyboardEvent, id: number) => {
    if (e.isComposing || e.altKey || e.defaultPrevented) return;
    const modifier = e.ctrlKey || e.metaKey;
    const index = this.subtitles.snapshot.cues.findIndex((c) => c.id === id);
    const targetIndex =
      e.key === 'ArrowUp'
        ? index - 1
        : e.key === 'ArrowDown'
          ? index + 1
          : e.key === 'Home'
            ? 0
            : e.key === 'End'
              ? this.subtitles.snapshot.cues.length - 1
              : null;
    if (modifier && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      void this.selectAllCues();
    } else if (modifier && e.code === 'Space') {
      e.preventDefault();
      void this.selectCue(id, 'toggle');
    } else if (targetIndex !== null) {
      e.preventDefault();
      const next =
        this.subtitles.snapshot.cues[
          clamp(targetIndex, 0, this.subtitles.snapshot.cues.length - 1)
        ];
      if (next && !this.workspace.busy && !this.workspace.confirming)
        void this.moveCueFocus(next.id, modifier, e.shiftKey);
    }
  };
  moveCueFocus = async (id: number, modifier: boolean, extend: boolean) => {
    if (!modifier || extend)
      await this.selectCue(
        id,
        extend ? (modifier ? 'addRange' : 'range') : 'replace',
      );
    await this.revealSelection(id);
    await tick();
    if (this.alive) this.effects.focusCue(id);
  };
  selectAllCues = async () => {
    if (
      this.workspace.busy ||
      this.workspace.confirming ||
      this.cueMenu?.context
    )
      return;
    await this.workspace.task(async () => {
      this.suspendFollowing();
      await this.subtitles.apply();
      this.subtitles.selectAll();
    });
  };
  requestProblem = (id: string) => {
    if (
      this.workspace.busy ||
      this.workspace.confirming ||
      this.cueMenu?.context ||
      !this.subtitles.snapshot.cues.some((c) => String(c.id) === id)
    )
      return;
    if (String(this.subtitles.selected) === id) return;
    this.suspendFollowing();
    if (this.subtitles.dirty) {
      this.pendingProblemVersion = `${this.subtitles.snapshot.generation}/${this.subtitles.snapshot.revision}`;
      this.pendingProblem = id;
    } else void this.selectCue(Number(id));
  };
  resolveProblem = async (choice: 'apply' | 'discard' | 'cancel') => {
    if (this.workspace.busy || !this.pendingProblem) return;
    const id = this.pendingProblem;
    if (choice === 'cancel') {
      this.pendingProblem = null;
      return;
    }
    const completed = await this.workspace.task(async () => {
      if (choice === 'apply') await this.subtitles.apply();
      else this.subtitles.revert();
      this.pendingProblem = null;
    });
    if (completed && this.alive) await this.selectCue(Number(id));
  };
  revealSelection = async (id: number | null = this.subtitles.selected) => {
    await tick();
    const viewport = this.effects.readList();
    if (!this.alive || !viewport) return;
    const index = this.subtitles.snapshot.cues.findIndex((c) => c.id === id);
    if (index >= 0)
      this.effects.scrollList(
        revealCueRow(index, viewport.scrollTop, viewport.height),
      );
  };
  suspendFollowing = () => {
    this.following = changeFollowing(
      this.following,
      'suspend',
      this.subtitles.dirty,
    );
  };

  setFollowing = (enabled: boolean) => {
    this.following = changeFollowing(
      this.following,
      enabled ? 'enable' : 'disable',
      this.subtitles.dirty || this.editorFocused || this.workspace.busy,
    );
  };
  resumeFollowing = () => {
    this.following = changeFollowing(
      this.following,
      'resume',
      this.subtitles.dirty || this.editorFocused,
    );
  };
  editAdjacent = async (action: 'nextCue' | 'previousCue') => {
    await this.workspace.task(async () => {
      await this.subtitles.apply();
      if (!this.alive) return;
      const doc = this.subtitles.snapshot;
      const index = doc.cues.findIndex(
        (cue) => cue.id === this.subtitles.selected,
      );
      const next =
        doc.cues[
          clamp(index + (action === 'nextCue' ? 1 : -1), 0, doc.cues.length - 1)
        ];
      if (next) {
        this.subtitles.select(next.id);
        await this.revealSelection();
        await this.focusText();
      }
    });
  };
  dispose = () => {
    this.alive = false;
  };
}
