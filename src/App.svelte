<script lang="ts">
  import WorkspaceDialogs from './lib/WorkspaceDialogs.svelte';
  import { WorkspaceDialogsState } from './lib/workspace-dialogs.svelte';
  import { Recovery } from './lib/recovery.svelte';
  import RecoveryBanner from './lib/RecoveryBanner.svelte';
  import { CueMenu } from './lib/cue-menu.svelte';
  import FontDiagnostics from './lib/FontDiagnostics.svelte';
  import PlaybackDiagnostics from './lib/PlaybackDiagnostics.svelte';
  import WorkspaceFooter from './lib/WorkspaceFooter.svelte';
  import WorkspaceToolbar from './lib/WorkspaceToolbar.svelte';
  import { runFileAction } from './lib/file-actions';
  import { runMediaAction, commandMedia } from './lib/media-actions';
  import { runCueAction } from './lib/cue-actions';
  import { runViewAction } from './lib/view-actions';
  import { workspaceKeydown } from './lib/workspace-keys';
  import CueTable from './lib/CueTable.svelte';
  import CueEditor from './lib/CueEditor.svelte';
  import WaveformPanel from './lib/WaveformPanel.svelte';
  import { CueTableView } from './lib/cue-table-view.svelte';
  import { Workspace } from './lib/workspace.svelte';
  import { measurePaneControl } from './lib/measure-pane-control';
  import { paneLayout } from './lib/pane-layout';

  import ProblemsPanel from './lib/ProblemsPanel.svelte';
  import TranslationPanel from './lib/TranslationPanel.svelte';
  import { TranslationSession } from './lib/translation-session.svelte';
  import LegacyImportDialog, {
    type LegacyImportChoice,
  } from './lib/LegacyImportDialog.svelte';
  import type { SelectionMode } from './lib/cue-selection';
  import { QualitySession } from './lib/quality-session.svelte';
  import { isTauri } from '@tauri-apps/api/core';
  import { defaultTiming } from './lib/magnetic-timing';
  import {
    defaultPreferences,
    getPreferences,
    type Preferences,
  } from './lib/client';
  import { DocumentSession } from './lib/document-session.svelte';
  import { MediaSession } from './lib/media-session.svelte';
  import { ProjectWorkflow } from './lib/project-workflow.svelte';
  import {
    WorkspaceModal,
    type AuthoringModal,
  } from './lib/workspace-modal.svelte';
  import { TimingGesture } from './lib/timing-gesture.svelte';
  import { TimelineView } from './lib/timeline-view.svelte';
  import { WorkspaceDecision } from './lib/workspace-decision.svelte';
  import { onMount, tick, untrack } from 'svelte';
  import {
    textFormat,
    type TextFormat,
    chooseFile,
    workspaceActions,
    updateMenu,
    type ProjectReferenceIssue,
    type DocumentAction,
    type DocumentSnapshot,
    type WorkspaceAction,
  } from './lib/client';
  import { clamp, formatTime, parseTime } from './lib/timeline';

  import CueContextMenu from './lib/CueContextMenu.svelte';
  import {
    cueOperationLabels,
    planCueOperation,
    type CueOperation,
    type CueOperationPlan,
  } from './lib/cue-operations';
  import {
    changeFollowing,
    followedSelection,
    revealCueRow,
    type CueFollowing,
  } from './lib/cue-following';
  import Splitter from './lib/Splitter.svelte';
  import ProjectSaveFailureDialog, {
    type ProjectExitChoice,
  } from './lib/ProjectSaveFailureDialog.svelte';
  import ProjectReferenceDialog from './lib/ProjectReferenceDialog.svelte';
  const workspace: Workspace = new Workspace({
    activity: () =>
      workspaceModal.opening ||
      gestures.active ||
      cueMenu.active ||
      recovery.active,
    confirming: () =>
      legacyDecision.visible ||
      legacyDecision.closing ||
      subtitleDecision.active ||
      projectExitDecision.active ||
      projectReferenceDecision.active ||
      workspaceModal.active,
    run: (action, event) => run(action, event),
  });
  const subtitles = new DocumentSession();
  const translation = new TranslationSession(subtitles);
  let translationOpen = $state(false);
  const quality = new QualitySession();
  let problemsOpen = $state(false);
  let showQualityMetrics = $state(false);
  let qualityContext = $state(0);
  let pendingProblem = $state<string | null>(null);
  let pendingProblemVersion = '';
  const media = new MediaSession((failure) => {
    if (alive) workspace.error = String(failure);
  });
  const timeline: TimelineView = new TimelineView({
    status: () => status,
    mediaPath: () => mediaPath,
    follow: () => follow,
    preview: () => gestures.edit?.preview ?? null,
    gesture: () => waveformGesture,
    pending: () => media.pending,
    cues: () => doc.cues,
    gap: () => timingSettings.cueGapMs,
    selectRange: (session, next) => media.loop.update(session, next),
  });
  const gestures: TimingGesture = new TimingGesture(
    subtitles,
    media,
    timeline,
    workspace,
    {
      menu: () => cueMenu.context,
      cuts: () => sceneCuts,
      settings: () => timingSettings,
      operationContext: () => operationContext,
      command: docAct,
      suspendFollowing: suspendCueFollowing,
      revealSelection,
      focusWaveform,
    },
  );
  const tableView = new CueTableView({
    cues: () => doc.cues,
    position: () =>
      status.loaded && !media.pending && !status.seeking
        ? status.positionSeconds
        : null,
  });
  const status = $derived(media.status);
  const doc = $derived(subtitles.snapshot);
  const recovery: Recovery = new Recovery(workspace, subtitles, {
    protect,
    suspendFollowing: suspendCueFollowing,
  });
  let textEditor = $state<HTMLTextAreaElement>();
  let waveHost = $state<HTMLDivElement>();
  function focusWaveform() {
    waveHost?.querySelector('canvas')?.focus();
  }
  async function focusText() {
    suspendCueFollowing();
    await tick();
    if (alive && cue) textEditor?.focus();
  }
  const cueMenu: CueMenu = new CueMenu(
    workspace,
    subtitles,
    media,
    timeline,
    gestures,
    {
      occlude: (value) => media.occlude(value),
      remainingOcclusion: () => diagnostics || workspace.confirming,
      afterRender: tick,
      captureFocus: () => {
        const target = document.activeElement;
        return () => {
          if (target instanceof HTMLElement && target.isConnected)
            target.focus();
        };
      },
      operationContext: () => operationContext,
      suspendFollowing: suspendCueFollowing,
      revealSelection,
      focusText,
      focusWaveform,
    },
  );
  const workspaceModal = new WorkspaceModal(workspace, {
    occlude: (value) => media.occlude(value),
    remainingOcclusion: () => diagnostics,
    afterRender: tick,
    captureFocus: () => {
      const target = document.activeElement;
      return () => {
        if (target instanceof HTMLElement && target.isConnected) target.focus();
      };
    },
  });
  let textFocused = $state(false);
  let follow = $state(true);
  let waveformGesture = $state(false);
  let cueFollowing = $state<CueFollowing>('following');
  let editorFocused = $state(false);
  let lastRevealedCue = '';
  let mediaPath = $state('');
  let restoredScenes = $state<{ session: number; generation: number } | null>(
    null,
  );
  let showScenes = $state(true);
  let preferences = $state<Preferences>({
    ...defaultPreferences,
    timing: { ...defaultTiming },
  });
  let legacyImportOpen = $state(false);
  let legacyImportReturn: HTMLElement | null = null;
  const nativeMenus = isTauri();
  let sceneControls = $state<{
    rescan: () => Promise<void>;
    cancel: () => Promise<void>;
    navigate: (backwards: boolean) => void;
  }>();
  let sceneState = $state({
    qualityKey: '',
    running: false,
    pending: false,
    previous: false,
    next: false,
  });

  async function closeLegacyImport(outputPath?: string, open = false) {
    if (outputPath) workspace.notice = `Project created at ${outputPath}`;
    await workspace.task(
      async () => {
        await legacyDecision.answer({ outputPath, open });
        const choice = await legacyDecision.wait();
        if (!alive || !choice) return;
        // Restoration completes before entering the existing protected open flow.
        if (choice.open && choice.outputPath)
          await projects.openPath(choice.outputPath);
      },
      { clearError: false },
    );
    if (alive) {
      await tick();
      if (alive && legacyImportReturn?.isConnected) legacyImportReturn.focus();
    }
  }

  async function formatText(style: TextFormat) {
    if (workspace.busy || workspace.confirming || !cue || !textEditor) return;
    const editor = textEditor;
    const before = subtitles.text;
    const beforeStyle = subtitles.style;
    const snapshot = doc;
    const id = selected;
    const start = textEditor.selectionStart;
    const end = textEditor.selectionEnd;
    await workspace.task(async () => {
      const result = await textFormat(before, start, end, style, {
        generation: snapshot.generation,
        revision: snapshot.revision,
        cueId: cue.id,
        styleName: beforeStyle,
      });
      if (
        !alive ||
        doc !== snapshot ||
        selected !== id ||
        subtitles.style !== beforeStyle ||
        subtitles.text !== before
      )
        return;
      subtitles.text = result.text;
      await tick();
      editor.focus();
      editor.setSelectionRange(result.start, result.end);
      workspace.notice =
        'Formatting updated in the cue draft. Apply edit to accept it.';
    });
  }

  const dialogs: WorkspaceDialogsState = new WorkspaceDialogsState(
    workspace,
    workspaceModal,
    subtitles,
    media,
    quality,
    cueMenu,
    {
      get preferences() {
        return preferences;
      },
      set preferences(value) {
        preferences = value;
      },
      get timingSettings() {
        return timingSettings;
      },
      set timingSettings(value) {
        timingSettings = value;
      },
      get showQualityMetrics() {
        return showQualityMetrics;
      },
      set showQualityMetrics(value) {
        showQualityMetrics = value;
      },
      get qualityContext() {
        return qualityContext;
      },
      set qualityContext(value) {
        qualityContext = value;
      },
      get problemsOpen() {
        return problemsOpen;
      },
      set problemsOpen(value) {
        problemsOpen = value;
      },
      get sceneState() {
        return sceneState;
      },
      get mediaPath() {
        return mediaPath;
      },
      get waveformGesture() {
        return waveformGesture;
      },
    },
    {
      requestProblem,
      selectCue,
      suspendFollowing: suspendCueFollowing,
      hasTextEditor: () => !!textEditor,
      selectText: (start, end) => {
        textEditor?.focus();
        textEditor?.setSelectionRange(start, end);
      },
    },
  );
  let hardware = $state(true);
  const selected = $derived(subtitles.selected);
  const cue = $derived(subtitles.cue);
  const draftDirty = $derived(subtitles.dirty);
  let diagnostics = $state(false);
  let video: HTMLDivElement;
  let workspaceHeight = $state(600);
  let topPct = $state(32);
  let editorPct = $state(25);
  let wavePct = $state(65);
  let editorBarHeight = $state(0);
  let editorFooterHeight = $state(0);
  let qualityReadoutHeight = $state(0);
  let transportHeight = $state(0);
  let cueOperationsHeight = $state(0);
  let cueFollowingHeight = $state(0);
  const panes = $derived(
    paneLayout({
      workspaceHeight,
      topPct,
      editorPct,
      showQualityMetrics,
      hasCue: !!cue,
      editorBarHeight,
      editorFooterHeight,
      qualityReadoutHeight,
      transportHeight,
      cueOperationsHeight,
      cueFollowingHeight,
    }),
  );
  const operationContext = $derived({
    timeStepMs: doc.format?.timeStepMs ?? 1,
    cues: doc.cues,
    selected,
    loaded: doc.loaded,
    draftDirty,
    positionSeconds:
      status.loaded && !status.seeking && !media.pending
        ? status.positionSeconds
        : null,
    range: timeline.range,
  });
  const cueOperations = $derived(
    (Object.entries(cueOperationLabels) as [CueOperation, string][]).map(
      ([operation, label]) => {
        try {
          return {
            operation,
            label,
            plan: planCueOperation(
              operation,
              operation === 'createCue'
                ? { ...operationContext, draftDirty: false }
                : operationContext,
            ),
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
    ),
  );
  let timingSettings = $state({ ...defaultTiming });
  let sceneCuts = $state<number[]>([]);
  let list = $state<HTMLDivElement>();
  const draftMetricInput = $derived.by(() => {
    if (!draftDirty || !cue) return { value: null, error: '' };
    try {
      return {
        value: {
          cueId: cue.id,
          startMs: parseTime(subtitles.start),
          endMs: parseTime(subtitles.end),
          text: subtitles.text,
        },
        error: '',
      };
    } catch (failure) {
      return { value: null, error: String(failure) };
    }
  });
  $effect(() => {
    quality.configure({
      session: status.session,
      enabled:
        workspace.ready && doc.loaded && (problemsOpen || showQualityMetrics),
      generation: doc.generation,
      revision: doc.revision,
      context: `${status.session}/${qualityContext}/${sceneState.qualityKey}`,
      selected,
      metricsOffset: tableView.firstRow,
      metricsLimit: showQualityMetrics ? tableView.visibleCues.length : 0,
      draft: showQualityMetrics ? draftMetricInput.value : null,
    });
  });
  $effect(() => {
    if (
      pendingProblem &&
      (pendingProblemVersion !== `${doc.generation}/${doc.revision}` ||
        !doc.cues.some((c) => String(c.id) === pendingProblem))
    )
      pendingProblem = null;
  });
  let confirmation: HTMLDialogElement;
  function decisionEffects(close: () => void) {
    return {
      occlude: (value: boolean) => media.occlude(value),
      remainingOcclusion: () => diagnostics,
      close,
      afterRender: tick,
    };
  }
  const subtitleDecision = new WorkspaceDecision<'save' | 'discard' | 'cancel'>(
    'cancel',
    workspace,
    decisionEffects(() => confirmation.close()),
  );
  const legacyDecision = new WorkspaceDecision<LegacyImportChoice | null>(
    null,
    workspace,
    decisionEffects(() => {
      legacyImportOpen = false;
    }),
  );
  let projectExitError = $state<string | null>(null);
  const projectExitDecision = new WorkspaceDecision<ProjectExitChoice>(
    'cancel',
    workspace,
    decisionEffects(() => {
      projectExitError = null;
    }),
  );
  let projectReferenceIssue = $state<ProjectReferenceIssue | null>(null);
  const projectReferenceDecision = new WorkspaceDecision<
    'cancel' | 'choose' | 'reload'
  >(
    'cancel',
    workspace,
    decisionEffects(() => {
      projectReferenceIssue = null;
    }),
  );
  let lastMenu = '';
  let alive = true;
  function docAct(action: DocumentAction, plan?: CueOperationPlan) {
    return subtitles.command(action, plan ? { plan } : {});
  }
  async function save(as: boolean) {
    const saved = await subtitles.save(as);
    if (saved) workspace.notice = 'Subtitles saved';
    return saved;
  }
  const projects: ProjectWorkflow = new ProjectWorkflow(subtitles, media, {
    read: () => ({
      mediaPath,
      hardware,
      options: {
        waveformStart: timeline.start,
        waveformSpan: timeline.span,
        followPlayback: follow,
        followSubtitle: cueFollowing !== 'off',
        showScenes,
        timing: timingSettings,
      },
    }),
    beginRestore: () => {
      // Suspend analysis until the newly opened native session has a path.
      mediaPath = '';
      restoredScenes = null;
      timeline.reset();
    },
    mediaOpened: (path) => {
      mediaPath = path;
      // Rust resets the policy on media replacement; keep the projection on
      // defaults until project attachment accepts its saved policy.
      timingSettings = { ...preferences.timing };
    },
    restoreOptions: (options) => {
      ++qualityContext;
      follow = options.followPlayback;
      cueFollowing = options.followSubtitle ? 'following' : 'off';
      showScenes = options.showScenes;
      timingSettings = { ...(options.timing ?? preferences.timing) };
      timeline.restore(options.waveformStart, options.waveformSpan);
    },
    restoreScenes: (session, generation) => {
      restoredScenes = { session, generation };
    },
    protect,
    saveSubtitles: () => save(false),
    saveFailed: projectSaveFailed,
    referenceIssue: resolveProjectReference,
    notify: (message) => {
      workspace.notice = message;
    },
  });
  async function protect(
    saveContext?: () => Promise<boolean>,
  ): Promise<boolean> {
    if (!alive) return false;
    if (!doc.dirty && !draftDirty && !doc.translation?.dirty) return true;
    const choice = await subtitleDecision.ask(() => confirmation.showModal());
    if (!alive || choice === 'cancel') return false;
    if (choice === 'save') {
      return doc.translation?.dirty
        ? (saveContext ?? (() => projects.save(false)))()
        : save(false);
    }
    return true;
  }
  async function referenceOperation(
    operation: 'attach' | 'copy' | 'offset' | 'detach',
    offsetMs = 0,
  ) {
    if (!workspace.canAct) return;
    await workspace.task(async () => {
      if (operation === 'attach' || operation === 'copy') {
        const path = await chooseFile(
          'subtitles',
          doc.translation?.reference?.path ?? doc.path,
        );
        if (!alive || !path) return;
        if (operation === 'copy' && !(await protect())) return;
        if (!alive) return;
        if (await translation.load(path, operation === 'copy')) {
          workspace.notice =
            operation === 'copy'
              ? 'Independent translation target created · Save As required'
              : 'Reference attached';
          ++qualityContext;
        }
      } else if (operation === 'offset') await translation.setOffset(offsetMs);
      else await translation.detach();
    });
  }
  $effect(() => {
    if (!translationOpen) translation.request(doc, null, 0, null, false);
  });
  function resolveProjectReference(issue: ProjectReferenceIssue) {
    return projectReferenceDecision.ask(() => {
      projectReferenceIssue = issue;
    });
  }
  function projectSaveFailed(failure: unknown): Promise<ProjectExitChoice> {
    return projectExitDecision.ask(() => {
      projectExitError = String(failure);
    });
  }
  async function selectCue(
    id: number,
    mode: SelectionMode = 'replace',
    expected?: DocumentSnapshot,
  ) {
    // Rows use aria-disabled so transport commands do not drop keyboard focus.
    // Enforce the pending-operation guard here for both pointer and keyboard use.
    if (
      !alive ||
      workspace.busy ||
      cueMenu.context ||
      (expected && doc !== expected) ||
      !doc.cues.some((c) => c.id === id)
    )
      return;
    suspendCueFollowing();
    gestures.cancelHeld();
    await workspace.task(async () => {
      await subtitles.apply();
      if (!alive || (expected && doc !== expected)) return;
      subtitles.select(id, mode);
      const selectedCue = subtitles.cue;
      if (selectedCue)
        timeline.selectRange([
          selectedCue.startMs / 1000,
          selectedCue.endMs / 1000,
        ]);
      await revealSelection();
      if (!alive || (expected && doc !== expected)) return;
      if (cue && status.loaded)
        await commandMedia(
          { media, gestures },
          { kind: 'seek', seconds: cue.startMs / 1000 },
        );
    });
  }
  function cueSelectionKey(e: KeyboardEvent, id: number) {
    if (e.isComposing || e.altKey || e.defaultPrevented) return;
    const modifier = e.ctrlKey || e.metaKey;
    const index = doc.cues.findIndex((c) => c.id === id);
    const targetIndex =
      e.key === 'ArrowUp'
        ? index - 1
        : e.key === 'ArrowDown'
          ? index + 1
          : e.key === 'Home'
            ? 0
            : e.key === 'End'
              ? doc.cues.length - 1
              : null;
    if (modifier && e.key.toLowerCase() === 'a') {
      e.preventDefault();
      void selectAllCues();
    } else if (modifier && e.code === 'Space') {
      e.preventDefault();
      void selectCue(id, 'toggle');
    } else if (targetIndex !== null) {
      e.preventDefault();
      const next = doc.cues[clamp(targetIndex, 0, doc.cues.length - 1)];
      if (next && !workspace.busy && !workspace.confirming)
        void moveCueFocus(next.id, modifier, e.shiftKey);
    }
  }
  async function moveCueFocus(id: number, modifier: boolean, extend: boolean) {
    if (!modifier || extend)
      await selectCue(
        id,
        extend ? (modifier ? 'addRange' : 'range') : 'replace',
      );
    await revealSelection(id);
    await tick();
    if (alive)
      list?.querySelector<HTMLButtonElement>(`[data-cue-id="${id}"]`)?.focus();
  }
  async function selectAllCues() {
    if (workspace.busy || workspace.confirming || cueMenu.context) return;
    await workspace.task(async () => {
      suspendCueFollowing();
      await subtitles.apply();
      subtitles.selectAll();
    });
  }
  function requestProblem(id: string) {
    if (
      workspace.busy ||
      workspace.confirming ||
      cueMenu.context ||
      !doc.cues.some((c) => String(c.id) === id)
    )
      return;
    if (String(selected) === id) return;
    suspendCueFollowing();
    if (draftDirty) {
      pendingProblemVersion = `${doc.generation}/${doc.revision}`;
      pendingProblem = id;
    } else void selectCue(Number(id));
  }
  async function resolveProblem(choice: 'apply' | 'discard' | 'cancel') {
    if (workspace.busy || !pendingProblem) return;
    const id = pendingProblem;
    if (choice === 'cancel') {
      pendingProblem = null;
      return;
    }
    const completed = await workspace.task(async () => {
      if (choice === 'apply') await subtitles.apply();
      else subtitles.revert();
      pendingProblem = null;
    });
    if (completed && alive) await selectCue(Number(id));
  }
  const actionView = {
    get diagnostics() {
      return diagnostics;
    },
    set diagnostics(value) {
      diagnostics = value;
    },
    get translationOpen() {
      return translationOpen;
    },
    set translationOpen(value) {
      translationOpen = value;
    },
    get problemsOpen() {
      return problemsOpen;
    },
    set problemsOpen(value) {
      problemsOpen = value;
    },
    get showScenes() {
      return showScenes;
    },
    set showScenes(value) {
      showScenes = value;
    },
    get topPct() {
      return topPct;
    },
    set topPct(value) {
      topPct = value;
    },
    get editorPct() {
      return editorPct;
    },
    set editorPct(value) {
      editorPct = value;
    },
    get wavePct() {
      return wavePct;
    },
    set wavePct(value) {
      wavePct = value;
    },
    get sceneControls() {
      return sceneControls;
    },
    set sceneControls(value) {
      sceneControls = value;
    },
  };
  const actionMedia = {
    get mediaPath() {
      return mediaPath;
    },
    set mediaPath(value) {
      mediaPath = value;
    },
    get hardware() {
      return hardware;
    },
    set hardware(value) {
      hardware = value;
    },
    get preferences() {
      return preferences;
    },
    set preferences(value) {
      preferences = value;
    },
    get timingSettings() {
      return timingSettings;
    },
    set timingSettings(value) {
      timingSettings = value;
    },
    get restoredScenes() {
      return restoredScenes;
    },
    set restoredScenes(value) {
      restoredScenes = value;
    },
  };
  const legacyImport = {
    get returnFocus() {
      return legacyImportReturn;
    },
    set returnFocus(value) {
      legacyImportReturn = value;
    },
    get open() {
      return legacyImportOpen;
    },
    set open(value) {
      legacyImportOpen = value;
    },
  };
  const fileActions = {
    workspace,
    subtitles,
    projects,
    gestures,
    tableView,
    legacyDecision,
    protect,
    save,
    legacyImport,
    resetScroll: () => {
      if (list) list.scrollTop = 0;
    },
  };
  const mediaActions = {
    workspace,
    media,
    projects,
    gestures,
    timeline,
    view: actionMedia,
  };
  const cueActions = {
    workspace,
    subtitles,
    gestures,
    get operationContext() {
      return operationContext;
    },
    suspendFollowing: suspendCueFollowing,
    revealSelection,
    focusText,
  };
  const viewActions = {
    workspace,
    subtitles,
    media,
    gestures,
    workspaceModal,
    openAuthoring: (kind: AuthoringModal) => dialogs.openAuthoring(kind),
    view: actionView,
  };
  async function run(action: WorkspaceAction, event?: Event) {
    if (!workspace.canAct || cueMenu.context || waveformGesture) return;
    switch (action) {
      case 'closeSubtitles':
      case 'importLegacyProject':
      case 'openProject':
      case 'openSubtitles':
      case 'quit':
      case 'save':
      case 'saveAs':
      case 'saveProject':
      case 'saveProjectAs':
        return runFileAction(action, fileActions);
      case 'clearLoop':
      case 'closeMedia':
      case 'nextFrame':
      case 'openMedia':
      case 'play':
      case 'playRange':
      case 'previousFrame':
        return runMediaAction(action, mediaActions);
      case 'apply':
      case 'createCue':
      case 'deleteCue':
      case 'mergeNext':
      case 'mergePrevious':
      case 'redo':
      case 'splitCue':
      case 'undo':
        return runCueAction(action, cueActions, event);
      case 'cancelSceneScan':
      case 'convertSubtitles':
      case 'diagnostics':
      case 'nextScene':
      case 'plugins':
      case 'previousScene':
      case 'resetLayout':
      case 'resync':
      case 'scanScenes':
      case 'settings':
      case 'showScenes':
      case 'timingSettings':
      case 'translationReference':
        return runViewAction(action, viewActions);
    }
  }
  async function revealSelection(id: number | null = selected) {
    await tick();
    if (!alive || !list) return;
    const index = doc.cues.findIndex((c) => c.id === id);
    if (index >= 0)
      list.scrollTop = revealCueRow(index, list.scrollTop, list.clientHeight);
  }
  async function seek(time: number) {
    if (!status.loaded || media.pending || workspace.busy) return;
    try {
      await commandMedia({ media, gestures }, { kind: 'seek', seconds: time });
    } catch (e) {
      workspace.error = String(e);
    }
  }
  async function selectTrack(id: number) {
    if (workspace.busy) return;
    await workspace.task(
      async () => {
        await commandMedia({ media, gestures }, { kind: 'track', id });
        timeline.invalidateTrack();
      },
      { clearError: false },
    );
  }
  $effect(() => {
    const state = {
      diagnostics,
      showScenes,
      scenesRunning: sceneState.running,
      scenesPending: sceneState.pending,
      previousScene: sceneState.previous,
      nextScene: sceneState.next,
      available: status.available,
      media: status.loaded,
      document: doc.loaded,
      convert: doc.loaded && doc.format?.name !== 'vtt',
      resync: status.loaded && doc.cues.length > 0,
      undo: doc.canUndo || draftDirty || textFocused,
      redo: doc.canRedo || draftDirty || textFocused,
      range: !!timeline.range && timeline.range[1] > timeline.range[0],
      busy: !workspace.canAct || waveformGesture,
      createCue: !!cueOperations.find((op) => op.operation === 'createCue')
        ?.plan,
      deleteCue: !!cueOperations.find((op) => op.operation === 'deleteCue')
        ?.plan,
      splitCue: !!cueOperations.find((op) => op.operation === 'splitCue')?.plan,
      mergePrevious: !!cueOperations.find(
        (op) => op.operation === 'mergePrevious',
      )?.plan,
      mergeNext: !!cueOperations.find((op) => op.operation === 'mergeNext')
        ?.plan,
    };
    const key = JSON.stringify(state);
    if (key === lastMenu) return;
    lastMenu = key;
    void updateMenu(state).catch((e) => (workspace.error = String(e)));
  });
  function suspendCueFollowing() {
    cueFollowing = changeFollowing(cueFollowing, 'suspend', draftDirty);
  }
  $effect(() => {
    // Includes timing buttons and other draft changes that do not focus an input.
    if (draftDirty || editorFocused) suspendCueFollowing();
  });
  $effect(() => {
    const protectedEdit =
      draftDirty || editorFocused || !workspace.canAct || media.pending;
    const next = followedSelection(
      tableView.activeIds,
      selected,
      cueFollowing,
      protectedEdit,
    );
    if (cueFollowing !== 'following') lastRevealedCue = '';
    if (
      protectedEdit ||
      cueFollowing !== 'following' ||
      next === null ||
      !tableView.activeIds.includes(next)
    )
      return;
    const index = doc.cues.findIndex((item) => item.id === next);
    const key = `${doc.generation}:${next}:${tableView.height}`;
    untrack(() => {
      // Automatic selection never calls selectCue: that path commits and seeks.
      if (next !== selected) {
        subtitles.select(next);
      }
      if (list && key !== lastRevealedCue) {
        list.scrollTop = revealCueRow(index, list.scrollTop, tableView.height);
        tableView.scrollTop = list.scrollTop;
        lastRevealedCue = key;
      }
    });
  });
  onMount(() => {
    alive = true;
    let unlisten = () => {};
    void Promise.all([
      subtitles.command({ kind: 'status' }),
      getPreferences().catch((e) => {
        if (alive) workspace.error = String(e);
        return defaultPreferences;
      }),
    ])
      .then(([, saved]) => {
        if (alive) {
          preferences = { ...saved, timing: { ...saved.timing } };
          timingSettings = { ...saved.timing };
          showQualityMetrics = saved.showQualityMetrics;
        }
      })
      .catch((e) => {
        if (alive) workspace.error = String(e);
      })
      .finally(() => {
        if (alive) workspace.ready = true;
      });
    void workspaceActions((action) => void run(action))
      .then((stop) => {
        if (alive) unlisten = stop;
        else stop();
      })
      .catch((e) => (workspace.error = String(e)));
    let layoutPending = false;
    let lastLayout = '';
    async function layout() {
      if (!alive || layoutPending || !status.available) return;
      const rect = video.getBoundingClientRect();
      const bounds = {
        kind: 'layout' as const,
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        visible:
          status.loaded &&
          rect.bottom > 0 &&
          rect.right > 0 &&
          rect.top < innerHeight &&
          rect.left < innerWidth,
      };
      const key = JSON.stringify([
        bounds,
        innerWidth,
        innerHeight,
        devicePixelRatio,
      ]);
      if (key === lastLayout) return;
      layoutPending = true;
      try {
        await media.layout(bounds);
        if (alive) lastLayout = key;
      } catch (e) {
        if (alive) workspace.error = String(e);
      } finally {
        layoutPending = false;
      }
    }
    const observer = new ResizeObserver(() => void layout());
    observer.observe(video);
    const listObserver = new ResizeObserver(([entry]) => {
      if (entry) tableView.height = entry.contentRect.height;
    });
    if (list) listObserver.observe(list);
    window.addEventListener('resize', layout);
    window.addEventListener('scroll', layout, true);
    timeline.startPolling();
    const layoutTimer = setInterval(() => void layout(), 100);
    recovery.startPolling();
    media.startPolling();
    return () => {
      alive = false;
      workspace.dispose();
      dialogs.dispose();
      workspaceModal.dispose();
      projects.dispose();
      translation.dispose();
      subtitles.dispose();
      quality.dispose();
      media.dispose();
      legacyDecision.dispose();
      subtitleDecision.dispose();
      projectExitDecision.dispose();
      projectReferenceDecision.dispose();
      tableView.dispose();
      gestures.dispose();
      timeline.dispose();
      unlisten();
      observer.disconnect();
      listObserver.disconnect();
      clearInterval(layoutTimer);
      recovery.dispose();
      cueMenu.dispose();
      window.removeEventListener('resize', layout);
      window.removeEventListener('scroll', layout, true);
    };
  });
  const keys = {
    workspace,
    gestures,
    subtitles,
    media,
    timeline,
    quality,
    get menu() {
      return !!cueMenu.context;
    },
    get waveformGesture() {
      return waveformGesture;
    },
    get problemsOpen() {
      return problemsOpen;
    },
    windows: __WINDOWS__,
    openText: (mode: 'find' | 'replace') => dialogs.openText(mode),
    requestProblem,
    focusWaveform,
    focusText,
    revealSelection,
    seek,
  };
</script>

<svelte:window
  onblur={() => gestures.cancelHeld()}
  onkeyup={(e) => {
    if (e.code === 'Space' && gestures.held) {
      e.preventDefault();
      void gestures.releaseHeld();
    }
  }}
  onfocusin={(e) =>
    (textFocused =
      e.target instanceof HTMLInputElement ||
      e.target instanceof HTMLTextAreaElement)}
  onfocusout={() => (textFocused = false)}
  onkeydown={(event) => workspaceKeydown(event, keys)}
/>
<main
  aria-busy={workspace.busy || !workspace.ready}
  class:has-recovery={recovery.status.entries.length > 0 ||
    !!recovery.status.error}
  style:--workspace-height={`${workspaceHeight}px`}
  style:--workspace-min-height={`${148 + panes.tableMinimum + panes.editorMinimum}px`}
  style:--table-min-height={`${panes.tableMinimum}px`}
>
  <WorkspaceToolbar
    {workspace}
    {media}
    {subtitles}
    {projects}
    {nativeMenus}
    {diagnostics}
    bind:showScenes
    {sceneState}
  />
  <RecoveryBanner {recovery} {workspace} />

  <div
    class="workspace"
    bind:clientHeight={workspaceHeight}
    style:--top-height={`${panes.topPixels}px`}
    style:--editor-height={`${panes.editorPixels}px`}
  >
    <div class="top" style:--wave-width={`${wavePct}%`}>
      <WaveformPanel
        {timeline}
        {gestures}
        {subtitles}
        {media}
        {workspace}
        {preferences}
        {timingSettings}
        {restoredScenes}
        {mediaPath}
        bind:showScenes
        bind:follow
        bind:sceneCuts
        bind:sceneControls
        bind:waveHost
        onstate={(value) => (sceneState = value)}
        ongesture={(active) => (waveformGesture = active)}
        actions={{
          seek,
          selectCue,
          openCueMenu: (x, y, id, inRange) => cueMenu.open(x, y, id, inRange),
        }}
      />
      <Splitter
        label="Resize waveform and video"
        vertical
        value={wavePct}
        onchange={(v) => (wavePct = clamp(v, 35, 78))}
      />
      <section class="video-panel" aria-label="Video preview">
        <div class="panelbar">
          <strong>Video</strong><span class="filename" title={mediaPath}
            >{mediaPath.split(/[\\/]/).pop()}</span
          >
        </div>
        <div
          bind:this={video}
          class="video-space"
          class:covered={status.surfaceAboveWebview || !status.loaded}
          aria-label="Native video surface"
        >
          {#if !status.loaded}<p>
              {status.available
                ? 'Open media to preview video'
                : 'GPU playback is unavailable in this build'}
            </p>{/if}
        </div>
      </section>
    </div>
    <Splitter
      label="Resize preview and subtitle list"
      value={(panes.topPixels / panes.paneHeight) * 100}
      onchange={(v) => (topPct = clamp(v, 20, Math.min(55, 80 - editorPct)))}
    />
    <section class="subtitle-panel panel" aria-label="Subtitle table">
      <div
        class="transport"
        use:measurePaneControl={(height) => (transportHeight = height)}
      >
        <button
          disabled={!status.loaded || workspace.busy}
          onclick={() => void run('play')}
          >{status.paused ? 'Play' : 'Pause'}</button
        ><button
          disabled={!status.loaded || workspace.busy}
          onclick={() => void run('previousFrame')}
          aria-label="Previous frame">◀|</button
        ><button
          disabled={!status.loaded || workspace.busy}
          onclick={() => void run('nextFrame')}
          aria-label="Next frame">|▶</button
        ><output
          >{status.positionSeconds === null
            ? '—'
            : formatTime(status.positionSeconds * 1000)}</output
        ><label
          >Audio <select
            value={status.audioTrack ?? ''}
            disabled={!status.loaded || workspace.busy}
            onchange={(e) => void selectTrack(Number(e.currentTarget.value))}
            >{#each status.audioTracks as track (track.id)}<option
                value={track.id}>{track.title}</option
              >{/each}</select
          ></label
        ><span class="spacer"></span><button
          disabled={!timeline.range || !status.loaded || workspace.busy}
          onclick={() => void run('playRange')}>Loop range</button
        ><button
          disabled={!status.loaded || workspace.busy}
          onclick={() => void run('clearLoop')}>Clear loop</button
        >
      </div>
      <div
        class="cue-operationbar"
        aria-label="Cue operations"
        use:measurePaneControl={(height) => (cueOperationsHeight = height)}
      >
        {#each cueOperations as op (op.operation)}
          <button
            disabled={!op.plan || workspace.busy || workspace.confirming}
            title={op.reason ||
              (op.operation === 'splitCue'
                ? 'Split timing and duplicate the accepted text in both halves.'
                : op.label)}
            onclick={() => void run(op.operation)}>{op.label}</button
          >
        {/each}
      </div>
      <div
        class="cue-followbar"
        use:measurePaneControl={(height) => (cueFollowingHeight = height)}
      >
        <button
          disabled={workspace.busy || workspace.confirming || !doc.cues.length}
          onclick={() => void dialogs.openText()}>Find / Replace…</button
        >
        <button
          disabled={workspace.busy || workspace.confirming || !doc.cues.length}
          onclick={() => void dialogs.openTimingReview('gap')}
          >Enforce minimum gaps…</button
        >
        <button
          disabled={workspace.busy || workspace.confirming || !doc.cues.length}
          onclick={() => void dialogs.openTimingReview('scene')}
          >Repair scene timing…</button
        >
        <button
          disabled={workspace.busy || workspace.confirming || !doc.cues.length}
          onclick={() => void dialogs.openTimingReview('duration')}
          >Repair duration / speed…</button
        >
        <button
          disabled={workspace.busy || workspace.confirming || !doc.cues.length}
          onclick={() => void dialogs.openTimingReview('batch')}
          >Shift / align cues…</button
        >

        <button
          disabled={workspace.busy || workspace.confirming || !doc.loaded}
          onclick={() => void dialogs.openStatistics()}>Statistics…</button
        >
        <button
          disabled={workspace.busy || workspace.confirming || !doc.loaded}
          onclick={() => void dialogs.openAuthoring('plugins')}
          >Subtitle plugins…</button
        >
        <button
          aria-expanded={problemsOpen}
          onclick={() => {
            problemsOpen = !problemsOpen;
            if (problemsOpen) translationOpen = false;
            if (!problemsOpen) pendingProblem = null;
          }}
          >Problems{quality.view?.profileHash
            ? ` · ${quality.view.findingsCount}`
            : ''}</button
        >
        <button
          disabled={!workspace.canAct}
          aria-expanded={translationOpen}
          onclick={() => {
            translationOpen = !translationOpen;
            if (translationOpen) problemsOpen = false;
          }}>Translation reference{doc.translation?.dirty ? ' •' : ''}</button
        >
        <label
          ><input type="checkbox" bind:checked={showQualityMetrics} /> Cue metrics</label
        >
        <label
          title="Tap Space to play/pause. While playing, hold Space for at least 400 ms to create a cue and keep timing. Enter edits text; Escape returns to the waveform."
        >
          <input
            type="checkbox"
            checked={gestures.mode}
            disabled={workspace.busy || workspace.confirming}
            onchange={(e) => {
              gestures.cancelHeld();
              gestures.mode = e.currentTarget.checked;
              focusWaveform();
            }}
          />
          Timing mode · hold Space
        </label>
        <label
          ><input
            type="checkbox"
            checked={cueFollowing !== 'off'}
            onchange={(e) => {
              cueFollowing = changeFollowing(
                cueFollowing,
                e.currentTarget.checked ? 'enable' : 'disable',
                draftDirty || editorFocused || workspace.busy,
              );
            }}
          /> Follow active subtitle</label
        >
        {#if cueFollowing === 'suspended'}
          <span
            >{draftDirty
              ? 'Apply or revert the draft to resume.'
              : 'Following suspended.'}</span
          >
          <button
            disabled={draftDirty || workspace.busy || workspace.confirming}
            onclick={() => {
              cueFollowing = changeFollowing(
                cueFollowing,
                'resume',
                draftDirty || editorFocused,
              );
            }}>Resume following</button
          >
        {/if}
      </div>
      <div
        class="subtitle-review"
        class:with-problems={problemsOpen || translationOpen}
      >
        <CueTable
          {subtitles}
          {quality}
          {tableView}
          {workspace}
          metrics={showQualityMetrics}
          bind:list
          actions={{
            suspendCueFollowing,
            openCueMenu: (x, y, id, inRange) => cueMenu.open(x, y, id, inRange),
            selectCue,
            cueSelectionKey,
            focusText,
          }}
        />
        {#if problemsOpen}<ProblemsPanel
            session={quality}
            disabled={workspace.busy || workspace.confirming}
            pending={pendingProblem !== null}
            onnavigate={requestProblem}
            onlanguage={(operation) => void dialogs.openText(operation)}
            onresolve={(choice) => void resolveProblem(choice)}
            onclose={() => {
              problemsOpen = false;
              pendingProblem = null;
            }}
          />{/if}
        {#if translationOpen}<TranslationPanel
            {subtitles}
            session={translation}
            disabled={!workspace.canAct}
            onload={(copy) => void referenceOperation(copy ? 'copy' : 'attach')}
            onoffset={(offset) => void referenceOperation('offset', offset)}
            ondetach={() => void referenceOperation('detach')}
            onclose={() => (translationOpen = false)}
          />{/if}
      </div>
    </section>
    <Splitter
      label="Resize subtitle list and editor"
      value={100 - (panes.editorPixels / panes.paneHeight) * 100}
      onchange={(v) =>
        (editorPct = clamp(100 - v, 20, Math.min(40, 80 - topPct)))}
    />
    <CueEditor
      {subtitles}
      {media}
      {quality}
      {workspace}
      metrics={showQualityMetrics}
      draftMetricError={draftMetricInput.error}
      bind:textEditor
      bind:editorFocused
      onmeasure={(part, height) => {
        if (part === 'bar') editorBarHeight = height;
        else if (part === 'footer') editorFooterHeight = height;
        else qualityReadoutHeight = height;
      }}
      actions={{
        suspendCueFollowing,
        openAuthoring: (kind: AuthoringModal) => dialogs.openAuthoring(kind),
        formatText,
      }}
    />
  </div>
  <FontDiagnostics {media} />
  <details class="shortcut-help">
    <summary>Keyboard shortcuts</summary>
    <p>
      ⌘/Ctrl+Alt+Enter: play/pause while typing (Alt+Space also works when the
      OS allows it) · Alt+←/→: seek 1 s (Shift: 5 s) · Alt+↑/↓: edit
      previous/next cue · In cue rows: Shift+click/arrows selects a range;
      Ctrl/Cmd+click or Space toggles, Ctrl/Cmd+arrows moves focus, Ctrl/Cmd+A
      selects all · Enter: edit selected cue · Escape: return to waveform /
      cancel held timing. In Timing mode, tap Space to play/pause; hold it for
      at least 400 ms during playback to create a cue and keep timing. Press
      Enter to edit its text.
    </p>
    <p>
      ⌘/Ctrl+Shift+N: create cue · ⌘/Ctrl+Shift+D: delete cue · ⌘/Ctrl+Enter:
      apply text · ⌘/Ctrl+S: save. Right-click the waveform selection or a cue
      for actions. Plain Space remains a text character in the editor.
    </p>
  </details>
  <WorkspaceFooter {workspace} {media} {gestures} {timeline} />
  <PlaybackDiagnostics
    {workspace}
    {media}
    {timeline}
    {diagnostics}
    bind:hardware
  />
  <dialog
    bind:this={confirmation}
    oncancel={(e) => {
      e.preventDefault();
      subtitleDecision.answer('cancel');
    }}
    aria-label={doc.translation?.dirty
      ? 'Unsaved project changes'
      : 'Unsaved subtitle changes'}
  >
    <h2>
      {doc.translation?.dirty
        ? 'Save project changes?'
        : 'Save subtitle changes?'}
    </h2>
    <p>
      {doc.translation?.dirty
        ? 'Save your subtitle changes and the translation reference and offset in a project.'
        : 'Your current document or cue draft has changes.'}
    </p>
    <div class="dialog-actions">
      <button onclick={() => subtitleDecision.answer('cancel')}>Cancel</button
      ><button onclick={() => subtitleDecision.answer('discard')}
        >Discard changes</button
      ><button onclick={() => subtitleDecision.answer('save')}
        >{doc.translation?.dirty ? 'Save project' : 'Save changes'}</button
      >
    </div>
  </dialog>
  {#if projectExitError !== null}
    <ProjectSaveFailureDialog
      error={projectExitError}
      onchoose={(choice) => void projectExitDecision.answer(choice)}
    />
  {/if}
  {#if projectReferenceIssue !== null}
    <ProjectReferenceDialog
      issue={projectReferenceIssue}
      onchoose={(choice) => void projectReferenceDecision.answer(choice)}
    />
  {/if}
  <WorkspaceDialogs
    {dialogs}
    {subtitles}
    {media}
    {mediaPath}
    {qualityContext}
    {sceneState}
    {preferences}
    {timingSettings}
  />
  {#if legacyImportOpen}
    <LegacyImportDialog
      canOpen={status.available}
      onclose={(path, open) => void closeLegacyImport(path, open)}
    />
  {/if}
  {#if cueMenu.context && cueMenu.items.length}
    {#key cueMenu.context}
      <CueContextMenu
        x={cueMenu.context.x}
        y={cueMenu.context.y}
        items={cueMenu.items}
        onchoose={(action) => void cueMenu.choose(action)}
        onclose={() => void cueMenu.close()}
      />
    {/key}
  {/if}
</main>

<style>
  :global(*) {
    box-sizing: border-box;
  }

  :global(body) {
    margin: 0;
    background: transparent;
    color: #e7eef3;
    font-family: system-ui, sans-serif;
    font-size: 13px;
  }

  :global(button),
  :global(input),
  :global(select),
  :global(textarea) {
    font: inherit;
    color: inherit;
  }

  :global(button) {
    border: 1px solid #49616f;
    border-radius: 4px;
    background: #263e4d;
    padding: 5px 9px;
    cursor: pointer;
    white-space: nowrap;
  }

  :global(button:hover:not(:disabled):not([aria-disabled='true'])) {
    background: #385765;
  }

  :global(button:disabled),
  :global(button[aria-disabled='true']) {
    opacity: 0.42;
    cursor: default;
  }

  :global(input),
  :global(select),
  :global(textarea) {
    border: 1px solid #49616f;
    border-radius: 3px;
    background: #12232e;
    padding: 4px 6px;
  }

  :global(:focus-visible) {
    outline: 2px solid #81ddc6;
    outline-offset: 1px;
  }

  :global(select) {
    /* Keep native controls legible alongside the workspace's light text. */
    color-scheme: dark;
  }

  main {
    height: 100dvh;
    min-height: 610px;
    display: grid;
    grid-template-rows: 48px minmax(var(--workspace-min-height), 1fr) auto 28px;
    isolation: isolate;
    overflow: auto;
  }

  main.has-recovery {
    grid-template-rows:
      48px auto minmax(var(--workspace-min-height), 1fr)
      auto 28px;
  }

  main:has(:global(.shortcut-help[open])) {
    min-height: 700px;
  }

  .shortcut-help {
    position: relative;
    z-index: 2;
    padding: 4px 12px;
    background: #192e3c;
    font-size: 12px;
    max-height: 120px;
    overflow: auto;
  }

  .shortcut-help summary {
    cursor: pointer;
  }

  .shortcut-help p {
    margin: 5px 0;
    line-height: 1.5;
  }

  .filename {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
    color: #b4c5d0;
  }

  .workspace {
    display: grid;
    grid-template-rows:
      var(--top-height) 6px minmax(var(--table-min-height), 1fr)
      6px var(--editor-height);
    min-height: 0;
    padding: 8px;
    gap: 0;
  }

  .top {
    display: grid;
    grid-template-columns: var(--wave-width) 6px minmax(150px, 1fr);
    min-height: 0;
  }

  .panel {
    background: #172934;
    position: relative;
    z-index: 2;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .panelbar {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 4px 8px;
    background: #203542;
    flex-shrink: 0;
  }

  .panelbar strong {
    font-size: 12px;
  }

  .spacer {
    flex: 1;
  }

  .video-panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
  }

  .video-panel .panelbar {
    z-index: 2;
  }

  .video-space {
    flex: 1;
    position: relative;
    min-height: 0;
    background: transparent;
    /* Cover the scrollable workspace while retaining the legacy video hole. */
    box-shadow: 0 0 0 max(100vmax, var(--workspace-height)) #14232d;
    z-index: 0;
  }

  .video-space p {
    position: absolute;
    inset: 20% 10%;
    text-align: center;
    color: #a7bdca;
  }

  .video-space.covered {
    /* Only the loaded legacy renderer needs a transparent webview hole. */
    background: #101e28;
  }

  .subtitle-panel {
    display: flex;
    flex-direction: column;
  }

  .subtitle-review {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .transport {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 8px;
    background: #203542;
    flex-wrap: wrap;
    flex-shrink: 0;
  }

  .transport output {
    font-variant-numeric: tabular-nums;
    min-width: 98px;
  }

  .transport label {
    display: flex;
    gap: 5px;
    align-items: center;
  }

  .transport select {
    max-width: 130px;
  }

  .cue-operationbar,
  .cue-followbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 4px 8px;
    font-size: 11px;
    background: #203542;
    flex-shrink: 0;
  }

  .cue-followbar label {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .cue-followbar button {
    padding: 2px 6px;
  }

  /* A scaled desktop can be smaller than the normal three-pane workspace.
     Keep every control reachable by scrolling instead of clipping fixed rows.
     This changes presentation only; drafts and accepted state stay mounted. */
  @media (max-width: 799px), (max-height: 609px) {
    main,
    main.has-recovery {
      height: auto;
      min-height: 100dvh;
      grid-template-rows: auto;
      overflow: visible;
    }

    .filename {
      flex-basis: 100%;
    }

    .workspace {
      grid-template-rows: auto minmax(320px, auto) minmax(210px, auto);
      gap: 8px;
    }

    .workspace > :global(.splitter) {
      display: none;
    }

    .top {
      height: 200px;
    }

    .subtitle-panel {
      height: 360px;
    }

    .subtitle-review {
      flex-direction: column;
    }
  }

  @media (max-width: 799px) {
    .top {
      height: auto;
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: 180px 180px;
      gap: 8px;
    }

    .top > :global(.splitter) {
      display: none;
    }
  }

  dialog {
    color: #e7eef3;
    background: #203542;
    border: 1px solid #81ddc6;
    border-radius: 8px;
    max-width: 440px;
    padding: 24px;
  }

  dialog::backdrop {
    background: #0009;
  }

  .dialog-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  h2 {
    margin-top: 0;
    font-size: 18px;
  }
</style>
