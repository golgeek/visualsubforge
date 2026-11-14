import { SettingsSession } from '../settings/settings-session.svelte';
import type { WorkspacePreferences } from '../settings/workspace-preferences.svelte';
import type { WorkspaceLayout } from '../layout/workspace-layout.svelte';
import {
  unavailable,
  isNativeAction,
  type ActionId,
  type ActionContext,
} from './action-registry';
import { editingKey } from './workspace-keys';
import type { Workspace } from './workspace.svelte';
import type { DocumentSession } from '../document/document-session.svelte';
import type { MediaSession } from '../media/media-session.svelte';
import type { WorkspaceScenes } from '../timeline/workspace-scenes.svelte';
import type { TimelineView } from '../timeline/timeline-view.svelte';
import type { WorkspaceView } from './workspace-view.svelte';
import type { QualitySession } from '../quality/quality-session.svelte';
import type { CueEditing } from '../cues/cue-editing.svelte';
import type { CueNavigation } from '../cues/cue-navigation.svelte';
import type { WorkspaceDialogsState } from '../dialogs/workspace-dialogs.svelte';
import type { WorkspaceModal } from '../dialogs/workspace-modal.svelte';
import type { TimingGesture } from '../timing/timing-gesture.svelte';
import type { CueMenu } from '../cues/cue-menu.svelte';
interface Owners {
  preferences: WorkspacePreferences;
  layout: WorkspaceLayout;
  workspace: Workspace;
  subtitles: DocumentSession;
  media: MediaSession;
  scenes: WorkspaceScenes;
  timeline: TimelineView;
  view: WorkspaceView;
  quality: QualitySession;
  cueEditing: CueEditing;
  navigation: CueNavigation;
  dialogs: WorkspaceDialogsState;
  workspaceModal: WorkspaceModal;
  gestures: TimingGesture;
  cueMenu: CueMenu;
  keys: () => Parameters<typeof editingKey>[1];
}

/** Discovery coordinates existing actions; the modal owns lifetime and Rust owns edits. */
export class WorkspaceCommands {
  constructor(private owners: Owners) {}
  get context(): ActionContext {
    const {
      workspace,
      subtitles,
      media,
      scenes,
      timeline,
      view,
      quality,
      cueEditing,
      cueMenu,
    } = this.owners;
    const doc = subtitles.snapshot;
    const status = media.status;
    return {
      checks: {
        themeSystem:
          this.owners.preferences.preferences.appearance.theme === 'system',
        themeDark:
          this.owners.preferences.preferences.appearance.theme === 'dark',
        themeLight:
          this.owners.preferences.preferences.appearance.theme === 'light',
        densityCompact:
          this.owners.preferences.preferences.appearance.density === 'compact',
        densityComfortable:
          this.owners.preferences.preferences.appearance.density ===
          'comfortable',
        translationReference: view.translationOpen,
        problems: view.problemsOpen && view.reviewTab === 'issues',
        statistics: view.problemsOpen && view.reviewTab === 'statistics',
        followPlayhead: timeline.follow,
        showScenes: scenes.showScenes,
        diagnostics: view.diagnostics,
        ...Object.fromEntries(
          ['timing', 'editorAbove', 'review', 'translation', 'bigPicture'].map(
            (preset) => [
              `layout${preset[0]!.toUpperCase()}${preset.slice(1)}`,
              !this.owners.layout.custom &&
                this.owners.layout.preset === preset,
            ],
          ),
        ),
      },
      ready: workspace.ready,
      busy: workspace.busy || timeline.interacting || !!cueMenu.context,
      document: doc.loaded,
      media: status.loaded,
      available: status.available,
      cue: !!subtitles.cue,
      cues: !!doc.cues.length,
      draft: subtitles.dirty,
      styled: !!subtitles.styled,
      webvtt: doc.format?.name === 'vtt',
      undo: doc.canUndo,
      redo: doc.canRedo,
      range: !!timeline.range && timeline.range[1] > timeline.range[0],
      scenesRunning: scenes.sceneState.running,
      scenesPending: scenes.sceneState.pending,
      previousScene: scenes.sceneState.previous,
      nextScene: scenes.sceneState.next,
      problems: view.problemsOpen,
      previousProblem: !!quality.view?.previousCue,
      nextProblem: !!quality.view?.nextCue,
      operations: Object.fromEntries(
        cueEditing.operations.map((op) => [
          op.operation,
          op.plan ? null : op.reason,
        ]),
      ),
    };
  }
  async open(mode: 'commands' | 'shortcuts' | 'actionMenu') {
    const { workspace, timeline, workspaceModal, gestures, cueMenu } =
      this.owners;
    if (!workspace.canAct || timeline.interacting || cueMenu.active) return;
    gestures.cancelHeld();
    await workspaceModal.open(mode);
  }
  async run(id: ActionId, fromNative = false) {
    const {
      workspace,
      timeline,
      view,
      quality,
      navigation,
      dialogs,
      workspaceModal,
    } = this.owners;
    await workspaceModal.close();
    if (workspace.disposed || !workspace.canAct) return;
    const reason = unavailable(id, this.context);
    if (reason) {
      workspace.notice = reason;
      return;
    }
    if (isNativeAction(id) && !fromNative) {
      await workspace.run(id, new Event('command'));
      return;
    }
    switch (id) {
      case 'themeSystem':
      case 'themeDark':
      case 'themeLight':
      case 'densityCompact':
      case 'densityComfortable': {
        const preferences = this.owners.preferences;
        await workspace.task(async () => {
          const session = new SettingsSession(
            preferences.preferences,
            (value) => {
              if (!workspace.disposed) preferences.accept(value);
            },
            () => null,
          );
          try {
            await session.load();
            if (workspace.disposed) return;
            if (!session.ready) throw new Error(session.error);
            switch (id) {
              case 'themeSystem':
                session.draft.appearance.theme = 'system';
                break;
              case 'themeDark':
                session.draft.appearance.theme = 'dark';
                break;
              case 'themeLight':
                session.draft.appearance.theme = 'light';
                break;
              case 'densityCompact':
                session.draft.appearance.density = 'compact';
                break;
              case 'densityComfortable':
                session.draft.appearance.density = 'comfortable';
                break;
            }
            if (!(await session.flush()))
              throw new Error(session.error || session.notice);
          } finally {
            session.dispose();
          }
        });
        break;
      }
      case 'followPlayhead':
        timeline.follow = !timeline.follow;
        timeline.resumeFollowing();
        break;
      case 'layoutOptions':
        await workspaceModal.open('layout');
        break;
      case 'layoutTiming':
        await this.owners.layout.select('timing');
        break;
      case 'layoutEditorAbove':
        await this.owners.layout.select('editorAbove');
        break;
      case 'layoutReview':
        await this.owners.layout.select('review');
        break;
      case 'layoutTranslation':
        await this.owners.layout.select('translation');
        break;
      case 'layoutBigPicture':
        await this.owners.layout.select('bigPicture');
        break;
      case 'textTools':
        await dialogs.openText();
        break;
      case 'find':
      case 'replace':
        await dialogs.openText(id);
        break;
      case 'fixGaps':
        await dialogs.openTimingReview('gap');
        break;
      case 'fixReadingSpeed':
        await dialogs.openTimingReview('duration');
        break;
      case 'snapShots':
        await dialogs.openTimingReview('scene');
        break;
      case 'shiftCues':
        await dialogs.openTimingReview('batch');
        break;
      case 'statistics':
        if (view.problemsOpen && view.reviewTab === 'statistics')
          view.closeProblems(() => {
            navigation.pendingProblem = null;
          });
        else await dialogs.openStatistics();
        break;
      case 'problems':
        view.toggleProblems(() => {
          navigation.pendingProblem = null;
        });
        break;
      case 'styles':
      case 'fonts':
        await dialogs.openAuthoring(id);
        break;
      case 'revertDraft':
        await this.owners.cueEditing.escape();
        break;
      case 'commandPalette':
        await this.open('commands');
        break;
      case 'shortcutSheet':
        await this.open('shortcuts');
        break;
      case 'previousProblem':
      case 'nextProblem': {
        view.openReview('issues');
        const cue =
          id === 'previousProblem'
            ? quality.view?.previousCue
            : quality.view?.nextCue;
        if (cue && !quality.stale) navigation.requestProblem(cue);
        break;
      }
      case 'zoomIn':
        timeline.zoom(0.5);
        break;
      case 'zoomOut':
        timeline.zoom(2);
        break;
      case 'fitWaveform':
        timeline.start = 0;
        timeline.span = timeline.duration;
        break;
      case 'focusText':
      case 'focusWaveform':
      case 'seekBack':
      case 'seekForward':
      case 'seekBackFar':
      case 'seekForwardFar':
      case 'previousCue':
      case 'nextCue':
      case 'setStart':
      case 'setEnd':
      case 'replay':
        await editingKey(id, this.owners.keys());
        break;
    }
  }
  async selectCue(id: number) {
    const { workspace, subtitles, navigation, workspaceModal } = this.owners;
    const expected = subtitles.snapshot;
    await workspaceModal.close();
    if (
      workspace.disposed ||
      !workspace.canAct ||
      subtitles.snapshot !== expected
    )
      return;
    await navigation.selectCue(id);
  }
}
