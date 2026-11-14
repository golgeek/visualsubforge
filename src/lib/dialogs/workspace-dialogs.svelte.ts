import { tick } from 'svelte';
import type { Preferences, TextOperation, DocumentSnapshot } from '../client';
import type {
  WorkspaceView,
  TimingKind,
} from '../workspace/workspace-view.svelte';
import type { WorkspacePreferences } from '../settings/workspace-preferences.svelte';
import type { TimelineView } from '../timeline/timeline-view.svelte';
import type { SelectionMode } from '../cues/cue-selection';
import type { Workspace } from '../workspace/workspace.svelte';
import type { WorkspaceModal, AuthoringModal } from './workspace-modal.svelte';
import type { DocumentSession } from '../document/document-session.svelte';
import type { QualitySession } from '../quality/quality-session.svelte';
import type { CueMenu } from '../cues/cue-menu.svelte';
interface Effects {
  requestProblem: (id: string) => void;
  selectCue: (
    id: number,
    mode: SelectionMode,
    expected: DocumentSnapshot,
  ) => Promise<void>;
  suspendFollowing: () => void;
  hasTextEditor: () => boolean;
  selectText: (start: number, end: number) => void;
}
export class WorkspaceDialogsState {
  textOperation = $state<TextOperation>({
    kind: 'find',
    query: '',
    caseSensitive: false,
  });
  private alive = true;
  constructor(
    private workspace: Workspace,
    readonly modal: WorkspaceModal,
    private subtitles: DocumentSession,
    private quality: QualitySession,
    private cueMenu: CueMenu,
    private view: WorkspaceView,
    private preferences: WorkspacePreferences,
    private timeline: TimelineView,
    private effects: Effects,
  ) {}
  dispose() {
    this.alive = false;
  }

  acceptSettings(value: Preferences) {
    if (!this.alive || this.modal.identity !== 'settings') return;
    this.preferences.accept(value);
    this.workspace.notice = 'Settings saved';
  }
  async closeSettings(value?: Preferences) {
    if (!this.alive || this.modal.identity !== 'settings') return;
    if (value) this.acceptSettings(value);
    await this.modal.close();
  }

  async closeResync(accepted = false) {
    if (!this.alive || this.modal.identity !== 'resync') return;
    if (accepted)
      this.workspace.notice =
        'Timings accepted. Use Save As to preserve the original subtitles.';
    await this.modal.close();
  }

  async openStatistics() {
    if (
      !this.workspace.canAct ||
      !this.subtitles.snapshot.loaded ||
      this.cueMenu.context ||
      this.timeline.interacting
    )
      return;
    this.workspace.error = '';
    this.view.openReview('statistics');
  }

  async openAuthoring(kind: AuthoringModal) {
    if (
      !this.workspace.canAct ||
      (kind === 'plugins'
        ? !this.subtitles.snapshot.loaded
        : kind === 'conversion'
          ? !this.subtitles.snapshot.loaded ||
            this.subtitles.snapshot.format?.name === 'vtt'
          : !this.subtitles.styled) ||
      this.timeline.interacting ||
      this.cueMenu.context
    )
      return;
    this.workspace.error = '';
    await this.modal.open(kind);
  }
  async closeAuthoring(accepted = false, outputPath?: string) {
    if (!this.alive) return;
    const kind =
      this.modal.identity === 'plugins'
        ? 'Plugin'
        : this.modal.identity === 'fonts'
          ? 'Font'
          : 'Style';
    if (outputPath)
      this.workspace.notice = `Converted subtitles exported to ${outputPath}`;
    if (accepted) {
      this.workspace.notice = `${kind} changes accepted. Undo restores the whole change.`;
      this.quality.restart();
    }
    await this.modal.close();
  }
  async openTimingReview(kind: TimingKind) {
    if (
      !this.workspace.canAct ||
      !this.subtitles.snapshot.cues.length ||
      this.cueMenu.context ||
      this.timeline.interacting
    )
      return;
    this.workspace.error = '';
    this.effects.suspendFollowing();
    this.view.openReview(
      kind === 'batch' ? 'shift' : 'timing',
      kind === 'batch' ? undefined : kind,
    );
  }
  async openText(mode?: 'find' | 'replace' | TextOperation) {
    if (
      !this.workspace.canAct ||
      !this.subtitles.snapshot.cues.length ||
      this.cueMenu.context ||
      this.timeline.interacting
    )
      return;
    this.workspace.error = '';
    if (typeof mode === 'object') {
      this.textOperation = mode;
    } else if (mode) {
      const old = this.textOperation;
      const query =
        old.kind === 'find' || old.kind === 'replace' ? old.query : '';
      const caseSensitive =
        old.kind === 'find' || old.kind === 'replace'
          ? old.caseSensitive
          : false;
      this.textOperation =
        mode === 'find'
          ? { kind: mode, query, caseSensitive }
          : {
              kind: mode,
              query,
              caseSensitive,
              replacement: old.kind === 'replace' ? old.replacement : '',
            };
    } else if (
      this.textOperation.kind === 'languageSuggestions' ||
      this.textOperation.kind === 'languageCorrection'
    ) {
      this.textOperation = { kind: 'find', query: '', caseSensitive: false };
    }
    await this.modal.open('text');
  }
  async closeText(operation: TextOperation, accepted = false) {
    if (!this.alive || this.modal.identity !== 'text') return;
    this.textOperation = operation;
    if (accepted) {
      this.workspace.notice =
        'Text changes accepted. Undo restores the whole change.';
      this.quality.restart();
    }
    await this.modal.close();
  }
  async navigateText(
    id: number,
    start: number,
    end: number,
    operation: TextOperation,
  ) {
    // The dialog resolves drafts before searching and checks capture identity
    // before navigation. Never apply an unexpected draft to leave a search row.
    if (this.subtitles.dirty || this.workspace.busy) return;
    const snapshot = this.subtitles.snapshot;
    await this.closeText(operation);
    if (
      !this.alive ||
      this.subtitles.dirty ||
      this.subtitles.snapshot !== snapshot
    )
      return;
    await this.effects.selectCue(id, 'replace', snapshot);
    await tick();
    if (
      this.alive &&
      this.subtitles.snapshot === snapshot &&
      !this.subtitles.dirty &&
      this.subtitles.selected === id &&
      this.effects.hasTextEditor()
    ) {
      this.effects.selectText(start, end);
    }
  }
}
