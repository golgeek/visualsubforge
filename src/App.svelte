<script lang="ts">
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

  import SettingsDialog from './lib/SettingsDialog.svelte';
  import ResyncDialog from './lib/ResyncDialog.svelte';
  import ProblemsPanel from './lib/ProblemsPanel.svelte';
  import TranslationPanel from './lib/TranslationPanel.svelte';
  import { TranslationSession } from './lib/translation-session.svelte';
  import StatisticsDialog from './lib/StatisticsDialog.svelte';
  import StyleInspector from './lib/StyleInspector.svelte';
  import FontDialog from './lib/FontDialog.svelte';
  import ConversionDialog from './lib/ConversionDialog.svelte';
  import PluginDialog from './lib/PluginDialog.svelte';
  import LegacyImportDialog, {
    type LegacyImportChoice,
  } from './lib/LegacyImportDialog.svelte';
  import GapDialog from './lib/GapDialog.svelte';
  import SceneRepairDialog from './lib/SceneRepairDialog.svelte';
  import DurationRepairDialog from './lib/DurationRepairDialog.svelte';
  import BatchTimingDialog from './lib/BatchTimingDialog.svelte';
  import type { SelectionMode } from './lib/cue-selection';
  import TextDialog from './lib/TextDialog.svelte';
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
    type TimingKind,
  } from './lib/workspace-modal.svelte';
  import { TimingGesture } from './lib/timing-gesture.svelte';
  import { TimelineView } from './lib/timeline-view.svelte';
  import { WorkspaceDecision } from './lib/workspace-decision.svelte';
  import { onMount, tick, untrack } from 'svelte';
  import {
    textFormat,
    type TextOperation,
    type TextFormat,
    chooseFile,
    workspaceActions,
    updateMenu,
    recoveryStatus,
    discardRecovery,
    type RecoveryStatus,
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
    activity: () => workspaceModal.opening || gestures.active,
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
      menu: () => cueMenu,
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
  let recovery = $state<RecoveryStatus>({ entries: [], error: null });
  let discardRecoveryId = $state<string | null>(null);
  let recoveryRequest = 0;
  let recoveryPolling = false;
  async function refreshRecovery() {
    if (recoveryPolling) return;
    recoveryPolling = true;
    const request = ++recoveryRequest;
    try {
      const next = await recoveryStatus();
      if (alive && request === recoveryRequest) recovery = next;
    } catch (e) {
      if (alive && request === recoveryRequest) recovery.error = String(e);
    } finally {
      recoveryPolling = false;
    }
  }
  async function chooseRecovery(id: string, recover: boolean) {
    if (!workspace.canAct) return;
    await workspace.task(async () => {
      if (recover) {
        if (!(await protect())) return;
        await docAct({ kind: 'recover', id, discard: true });
        suspendCueFollowing();
        workspace.notice =
          'Recovered as an unsaved copy. Save As chooses its destination; the recovery snapshot is retained.';
      } else {
        ++recoveryRequest;
        await discardRecovery(id);
        // A poll can start while discard is pending. Its result predates the
        // confirmed removal even if it arrives after this local update.
        ++recoveryRequest;
        recovery.entries = recovery.entries.filter((entry) => entry.id !== id);
        discardRecoveryId = null;
      }
    });
  }
  let textEditor = $state<HTMLTextAreaElement>();
  let waveHost = $state<HTMLDivElement>();
  let menuEpoch = 0;
  let menuReturn: HTMLElement | null = null;
  let cueMenu = $state<{
    x: number;
    y: number;
    cueId: number | null;
    range: [number, number] | null;
    generation: number;
    revision: number;
    session: number;
  } | null>(null);
  const menuItems = $derived(
    cueMenu
      ? [
          ...(cueMenu.range
            ? [{ id: 'create', label: 'Create cue from selection' }]
            : []),
          ...(cueMenu.cueId !== null
            ? [
                { id: 'edit', label: 'Edit cue text' },
                { id: 'delete', label: 'Delete cue' },
              ]
            : []),
        ]
      : [],
  );
  function focusWaveform() {
    waveHost?.querySelector('canvas')?.focus();
  }
  async function focusText() {
    suspendCueFollowing();
    await tick();
    if (alive && cue) textEditor?.focus();
  }
  async function closeCueMenu(restoreFocus = true) {
    ++menuEpoch;
    cueMenu = null;
    if (restoreFocus && menuReturn?.isConnected) menuReturn.focus();
    try {
      await media.occlude(diagnostics || workspace.confirming);
    } catch (e) {
      workspace.error = String(e);
    }
  }
  async function openCueMenu(
    x: number,
    y: number,
    cueId: number | null,
    inRange: boolean,
  ) {
    if (!workspace.canAct || (cueId === null && !inRange)) return;
    gestures.cancelHeld();
    const candidate = {
      x,
      y,
      cueId,
      range:
        inRange && timeline.range
          ? ([...timeline.range] as [number, number])
          : null,
      generation: doc.generation,
      revision: doc.revision,
      session: status.session,
    };
    const request = ++menuEpoch;
    menuReturn =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    await workspace.task(
      async () => {
        await media.occlude(true);
        if (alive && request === menuEpoch) cueMenu = candidate;
      },
      { clearError: false },
    );
  }
  async function chooseCueMenu(action: string) {
    const context = cueMenu;
    if (!context || workspace.busy) return;
    await workspace.task(async () => {
      await closeCueMenu(false);
      if (
        context.generation !== doc.generation ||
        context.revision !== doc.revision ||
        context.session !== status.session
      )
        throw new Error('The document or media changed; open the menu again.');
      await subtitles.apply();
      suspendCueFollowing();
      if (action === 'create' && context.range) {
        const plan = planCueOperation('createCue', {
          ...operationContext,
          range: context.range,
          draftDirty: false,
        });
        await docAct(plan.action, plan);
        await revealSelection();
        await focusText();
      } else if (context.cueId !== null) {
        if (!doc.cues.some((c) => c.id === context.cueId))
          throw new Error('This cue no longer exists.');
        if (action === 'delete') {
          const plan = planCueOperation('deleteCue', {
            ...operationContext,
            selected: context.cueId,
            draftDirty: false,
          });
          await docAct(plan.action, plan);
          focusWaveform();
        } else {
          subtitles.select(context.cueId);
          await revealSelection();
          await focusText();
        }
      }
    });
  }
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
  const timingLabels: Record<TimingKind, string> = {
    gap: 'Gap',
    scene: 'Scene',
    duration: 'Duration and speed',
    batch: 'Batch',
  };
  const TimingDialog = $derived(
    workspaceModal.timingKind === 'duration'
      ? DurationRepairDialog
      : workspaceModal.timingKind === 'scene'
        ? SceneRepairDialog
        : GapDialog,
  );
  let textOperation = $state<TextOperation>({
    kind: 'find',
    query: '',
    caseSensitive: false,
  });
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
  async function closeSettings(value?: Preferences) {
    if (!alive || workspaceModal.identity !== 'settings') return;
    if (value) {
      preferences = { ...value, timing: { ...value.timing } };
      timingSettings = { ...value.timing };
      showQualityMetrics = value.showQualityMetrics;
      ++qualityContext;
      workspace.notice = 'Settings saved';
    }
    await workspaceModal.close();
  }

  async function closeResync(accepted = false) {
    if (!alive || workspaceModal.identity !== 'resync') return;
    if (accepted)
      workspace.notice =
        'Timings accepted. Use Save As to preserve the original subtitles.';
    await workspaceModal.close();
  }

  async function openStatistics() {
    if (!workspace.canAct || !doc.loaded || cueMenu || waveformGesture) return;
    workspace.error = '';
    await workspaceModal.open('statistics');
  }

  async function openAuthoring(kind: AuthoringModal) {
    if (
      !workspace.canAct ||
      (kind === 'plugins'
        ? !doc.loaded
        : kind === 'conversion'
          ? !doc.loaded || doc.format?.name === 'vtt'
          : !subtitles.styled) ||
      waveformGesture ||
      cueMenu
    )
      return;
    workspace.error = '';
    await workspaceModal.open(kind);
  }
  async function closeAuthoring(accepted = false, outputPath?: string) {
    if (!alive) return;
    const kind =
      workspaceModal.identity === 'plugins'
        ? 'Plugin'
        : workspaceModal.identity === 'fonts'
          ? 'Font'
          : 'Style';
    if (outputPath)
      workspace.notice = `Converted subtitles exported to ${outputPath}`;
    if (accepted) {
      workspace.notice = `${kind} changes accepted. Undo restores the whole change.`;
      quality.restart();
    }
    await workspaceModal.close();
  }
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
  async function closeStatistics(id?: string) {
    if (!alive || workspaceModal.identity !== 'statistics') return;
    const version = `${doc.generation}/${doc.revision}/${status.session}/${qualityContext}/${sceneState.qualityKey}`;
    await workspaceModal.close();
    if (
      alive &&
      id &&
      version ===
        `${doc.generation}/${doc.revision}/${status.session}/${qualityContext}/${sceneState.qualityKey}`
    ) {
      problemsOpen = true;
      requestProblem(id);
    }
  }
  async function openTimingReview(kind: TimingKind) {
    if (!workspace.canAct || !doc.cues.length || cueMenu || waveformGesture)
      return;
    workspace.error = '';
    suspendCueFollowing();
    await workspaceModal.open({ kind: 'timing', variant: kind });
  }
  async function openText(mode?: 'find' | 'replace' | TextOperation) {
    if (!workspace.canAct || !doc.cues.length || cueMenu || waveformGesture)
      return;
    workspace.error = '';
    if (typeof mode === 'object') {
      textOperation = mode;
    } else if (mode) {
      const old = textOperation;
      const query =
        old.kind === 'find' || old.kind === 'replace' ? old.query : '';
      const caseSensitive =
        old.kind === 'find' || old.kind === 'replace'
          ? old.caseSensitive
          : false;
      textOperation =
        mode === 'find'
          ? { kind: mode, query, caseSensitive }
          : {
              kind: mode,
              query,
              caseSensitive,
              replacement: old.kind === 'replace' ? old.replacement : '',
            };
    } else if (
      textOperation.kind === 'languageSuggestions' ||
      textOperation.kind === 'languageCorrection'
    ) {
      textOperation = { kind: 'find', query: '', caseSensitive: false };
    }
    await workspaceModal.open('text');
  }
  async function closeText(operation: TextOperation, accepted = false) {
    if (!alive || workspaceModal.identity !== 'text') return;
    textOperation = operation;
    if (accepted) {
      workspace.notice =
        'Text changes accepted. Undo restores the whole change.';
      quality.restart();
    }
    await workspaceModal.close();
  }
  async function navigateText(
    id: number,
    start: number,
    end: number,
    operation: TextOperation,
  ) {
    // The dialog resolves drafts before searching and checks capture identity
    // before navigation. Never apply an unexpected draft to leave a search row.
    if (draftDirty || workspace.busy) return;
    const snapshot = doc;
    await closeText(operation);
    if (!alive || draftDirty || doc !== snapshot) return;
    await selectCue(id, 'replace', snapshot);
    await tick();
    if (
      alive &&
      doc === snapshot &&
      !draftDirty &&
      selected === id &&
      textEditor
    ) {
      textEditor.focus();
      textEditor.setSelectionRange(start, end);
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
  async function closeTimingReview(accepted = false) {
    if (!alive || workspaceModal.timingKind === null) return;
    if (accepted) {
      workspace.notice = `${timingLabels[workspaceModal.timingKind]} timings accepted. Undo restores the whole change.`;
      quality.restart();
    }
    await workspaceModal.close();
  }

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
      cueMenu ||
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
    if (workspace.busy || workspace.confirming || cueMenu) return;
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
      cueMenu ||
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
    openAuthoring,
    view: actionView,
  };
  async function run(action: WorkspaceAction, event?: Event) {
    if (!workspace.canAct || cueMenu || waveformGesture) return;
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
    const recoveryTimer = setInterval(() => void refreshRecovery(), 5000);
    void refreshRecovery();
    media.startPolling();
    return () => {
      alive = false;
      workspace.dispose();
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
      clearInterval(recoveryTimer);
      ++recoveryRequest;
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
      return !!cueMenu;
    },
    get waveformGesture() {
      return waveformGesture;
    },
    get problemsOpen() {
      return problemsOpen;
    },
    windows: __WINDOWS__,
    openText,
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
  class:has-recovery={recovery.entries.length > 0 || !!recovery.error}
  style:--workspace-height={`${workspaceHeight}px`}
  style:--workspace-min-height={`${148 + panes.tableMinimum + panes.editorMinimum}px`}
  style:--table-min-height={`${panes.tableMinimum}px`}
>
  <header class="toolbar">
    <strong>VisualSubForge</strong>
    {#if !nativeMenus}
      <button
        disabled={workspace.busy || !workspace.ready}
        onclick={() => void run('openProject')}>Open Project</button
      >
      <button
        disabled={!workspace.canAct}
        onclick={() => void run('importLegacyProject')}
        >Import VisualSubSync Project</button
      >
      <button
        disabled={workspace.busy ||
          !workspace.ready ||
          (!status.loaded && !doc.loaded)}
        title={projects.path ??
          'Save subtitle references, media, scene results and workspace options'}
        onclick={() => void run('saveProject')}>Save Project</button
      >
      <button
        disabled={workspace.busy || !workspace.ready || !status.available}
        onclick={() => void run('openMedia')}>Open Media</button
      >
      <button
        disabled={workspace.busy || !workspace.ready}
        onclick={() => void run('openSubtitles')}>Open Subtitles</button
      >
      <button
        disabled={workspace.busy || !doc.loaded}
        onclick={() => void run('save')}>Save</button
      >
    {/if}
    <span class="filename" title={doc.path ?? ''}
      >{doc.path?.split(/[\\/]/).pop() ??
        (doc.loaded ? 'Untitled subtitles' : 'No subtitles')}{doc.dirty ||
      draftDirty
        ? ' •'
        : ''}{doc.requiresSaveAs ? ' (Save As required)' : ''}</span
    >
    {#if !nativeMenus}
      <button
        class="quiet"
        onclick={() => void run('diagnostics')}
        aria-pressed={diagnostics}>Diagnostics</button
      >
      <button disabled={!workspace.canAct} onclick={() => void run('settings')}
        >Settings…</button
      >
      <button
        disabled={workspace.busy ||
          workspace.confirming ||
          !workspace.ready ||
          !status.loaded ||
          !doc.cues.length}
        onclick={() => void run('resync')}>Resynchronize subtitles…</button
      >
      <button
        disabled={!status.loaded ||
          workspace.busy ||
          workspace.confirming ||
          sceneState.pending}
        onclick={() =>
          void run(sceneState.running ? 'cancelSceneScan' : 'scanScenes')}
        >{sceneState.running
          ? 'Cancel scene scan'
          : 'Scan scene changes'}</button
      >
      <label
        ><input type="checkbox" bind:checked={showScenes} /> Show scene changes</label
      >
    {/if}
  </header>
  {#if recovery.entries.length || recovery.error}
    <aside class="recovery" aria-label="Subtitle recovery">
      {#if recovery.error}<p role="alert">Recovery: {recovery.error}</p>{/if}
      {#if recovery.entries.length}<p>
          Recovery snapshots are available. Recover opens an unsaved copy;
          originals stay unchanged.
        </p>{/if}
      {#each recovery.entries as entry (entry.id)}
        <div class="recovery-row">
          <span title={entry.sourcePath ?? entry.id}
            >{entry.sourcePath?.split(/[\\/]/).pop() ??
              (entry.error ? entry.id : 'Untitled subtitles')}
            {entry.capturedMs
              ? new Date(entry.capturedMs).toLocaleString()
              : ''}
            {entry.error ?? ''}</span
          >
          <button
            disabled={workspace.busy || !workspace.ready || !!entry.error}
            onclick={() => void chooseRecovery(entry.id, true)}>Recover</button
          >
          {#if discardRecoveryId === entry.id}
            <span>Discard this recovery snapshot?</span>
            <button
              disabled={workspace.busy}
              onclick={() => void chooseRecovery(entry.id, false)}
              >Confirm discard</button
            >
            <button
              disabled={workspace.busy}
              onclick={() => (discardRecoveryId = null)}>Cancel</button
            >
          {:else}
            <button
              disabled={workspace.busy}
              onclick={() => (discardRecoveryId = entry.id)}>Discard…</button
            >
          {/if}
        </div>
      {/each}
    </aside>
  {/if}
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
        actions={{ seek, selectCue, openCueMenu }}
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
          onclick={() => void openText()}>Find / Replace…</button
        >
        <button
          disabled={workspace.busy || workspace.confirming || !doc.cues.length}
          onclick={() => void openTimingReview('gap')}
          >Enforce minimum gaps…</button
        >
        <button
          disabled={workspace.busy || workspace.confirming || !doc.cues.length}
          onclick={() => void openTimingReview('scene')}
          >Repair scene timing…</button
        >
        <button
          disabled={workspace.busy || workspace.confirming || !doc.cues.length}
          onclick={() => void openTimingReview('duration')}
          >Repair duration / speed…</button
        >
        <button
          disabled={workspace.busy || workspace.confirming || !doc.cues.length}
          onclick={() => void openTimingReview('batch')}
          >Shift / align cues…</button
        >

        <button
          disabled={workspace.busy || workspace.confirming || !doc.loaded}
          onclick={() => void openStatistics()}>Statistics…</button
        >
        <button
          disabled={workspace.busy || workspace.confirming || !doc.loaded}
          onclick={() => void openAuthoring('plugins')}
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
            openCueMenu,
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
            onlanguage={(operation) => void openText(operation)}
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
      actions={{ suspendCueFollowing, openAuthoring, formatText }}
    />
  </div>
  {#if status.subtitleFontDiagnostics.length > 0}
    <details class="shortcut-help" aria-label="Subtitle font notices">
      <summary
        >Subtitle font notices ({status.subtitleFontDiagnostics
          .length})</summary
      >
      <p>
        The renderer reported font issues in this media session. Fallback may
        still render the text. Check the affected subtitles and make the
        required fonts available; original document font names are unchanged.
        These notices include previously displayed cues and clear when media is
        closed or replaced.
      </p>
      <ul>
        {#each status.subtitleFontDiagnostics as message, index (index)}<li>
            {message}
          </li>{/each}
      </ul>
    </details>
  {/if}
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
  <footer>
    <span
      class="message"
      role="status"
      title={workspace.error ||
        status.error ||
        status.subtitlePreviewError ||
        undefined}
      >{workspace.error ||
        status.error ||
        status.subtitlePreviewError ||
        workspace.notice ||
        (workspace.busy ? 'Working…' : 'Ready')}</span
    >{#if gestures.edit}<span
        >{gestures.snapHint || 'Timing preview'}
        {formatTime(gestures.edit.preview.startMs)}–{formatTime(
          gestures.edit.preview.endMs,
        )}</span
      >{/if}{#if timeline.range}<span
        >Range {timeline.range[0].toFixed(3)}–{timeline.range[1].toFixed(3)} s</span
      >{/if}<span>Desktop PoC</span>
  </footer>
  {#if diagnostics}<aside
      class="diagnostics panel"
      aria-label="Playback diagnostics"
    >
      <div class="panelbar">
        <strong>Playback diagnostics</strong><button
          onclick={() => void run('diagnostics')}>Close diagnostics</button
        >
      </div>
      <label
        >Decode on next Open <select bind:value={hardware}
          ><option value={true}>Prefer hardware (software fallback)</option
          ><option value={false}>Software</option></select
        ></label
      >
      <p>
        Decoder {status.hardwareDecoder || 'none'} · Drops {status.decoderDrops ??
          '—'} / {status.outputDrops ?? '—'} · GPU swaps {status.swapCounterAvailable
          ? status.renderedFrames
          : 'unavailable'} · Session {status.session} · Audio clock {status.queuedAudioClock
          ? 'queued timeline (experimental)'
          : 'legacy'}
      </p>
      <p>
        Waveform stream {timeline.wave?.audioStreamIndex ?? '—'} · container origin
        {timeline.wave?.containerOriginSeconds ?? '—'} s
      </p>
      <p>{timeline.error || timeline.wave?.error || ''}</p>
    </aside>{/if}
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
  {#if workspaceModal.identity === 'resync'}
    <ResyncDialog
      {subtitles}
      media={() => status}
      target={mediaPath}
      onclose={() => void closeResync()}
      onaccepted={() => void closeResync(true)}
    />
  {/if}
  {#if workspaceModal.identity === 'statistics'}
    <StatisticsDialog
      {subtitles}
      mediaSession={() => status.session}
      context={() => `${qualityContext}/${sceneState.qualityKey}`}
      onclose={() => void closeStatistics()}
      onnavigate={(id) => void closeStatistics(id)}
    />
  {/if}
  {#if workspaceModal.identity === 'fonts' && doc.format && subtitles.styled}
    <FontDialog
      {subtitles}
      onclose={() => void closeAuthoring()}
      onaccepted={() => void closeAuthoring(true)}
    />
  {/if}
  {#if workspaceModal.identity === 'plugins'}
    <PluginDialog
      {subtitles}
      media={() => status}
      context={() => `${qualityContext}/${sceneState.qualityKey}`}
      onclose={() => void closeAuthoring()}
      onaccepted={() => void closeAuthoring(true)}
    />
  {/if}
  {#if workspaceModal.identity === 'conversion'}
    <ConversionDialog
      {subtitles}
      onclose={(path) => void closeAuthoring(false, path)}
    />
  {/if}
  {#if legacyImportOpen}
    <LegacyImportDialog
      canOpen={status.available}
      onclose={(path, open) => void closeLegacyImport(path, open)}
    />
  {/if}
  {#if workspaceModal.identity === 'styles' && doc.format && subtitles.styled}
    <StyleInspector
      {subtitles}
      initial={subtitles.style}
      onclose={() => void closeAuthoring()}
      onaccepted={() => void closeAuthoring(true)}
    />
  {/if}
  {#if workspaceModal.timingKind !== null}
    {#if workspaceModal.timingKind === 'batch'}
      <BatchTimingDialog
        {subtitles}
        media={() => status}
        onclose={() => void closeTimingReview()}
        onaccepted={() => void closeTimingReview(true)}
      />
    {:else}
      <TimingDialog
        {subtitles}
        mediaSession={() => status.session}
        onclose={() => void closeTimingReview()}
        onaccepted={() => void closeTimingReview(true)}
      />
    {/if}
  {/if}
  {#if workspaceModal.identity === 'text'}
    <TextDialog
      {subtitles}
      initial={textOperation}
      onclose={(operation) => void closeText(operation)}
      onaccepted={(operation) => void closeText(operation, true)}
      onnavigate={(id, start, end, operation) =>
        void navigateText(id, start, end, operation)}
    />
  {/if}
  {#if workspaceModal.identity === 'settings'}
    <SettingsDialog
      {preferences}
      timing={timingSettings}
      onapply={(value) => void closeSettings(value)}
      onclose={() => void closeSettings()}
    />
  {/if}
  {#if cueMenu && menuItems.length}
    {#key cueMenu}
      <CueContextMenu
        x={cueMenu.x}
        y={cueMenu.y}
        items={menuItems}
        onchoose={(action) => void chooseCueMenu(action)}
        onclose={() => void closeCueMenu()}
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

  main:has(.shortcut-help[open]) {
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

  .recovery {
    position: relative;
    z-index: 2;
    max-height: 130px;
    overflow: auto;
    padding: 4px 12px;
    background: #304b54;
  }

  .recovery p {
    margin: 4px 0;
  }

  .recovery-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }

  .recovery-row > span:first-child {
    flex: 1;
    overflow-wrap: anywhere;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    background: #1e303d;
    z-index: 2;
    border-bottom: 1px solid #415560;
  }

  .toolbar strong {
    margin-right: 8px;
  }

  .filename {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
    color: #b4c5d0;
  }

  .quiet {
    background: transparent;
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

  .panelbar button {
    padding: 1px 8px;
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

  footer {
    min-width: 0;
    display: flex;
    gap: 16px;
    align-items: center;
    padding: 0 12px;
    background: #203542;
    z-index: 2;
    color: #adc4d1;
    font-size: 11px;
  }

  .message {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .diagnostics {
    position: absolute;
    right: 12px;
    top: 52px;
    width: min(520px, 90%);
    border: 1px solid #87b4bb;
    box-shadow: 0 8px 30px #0008;
    z-index: 5;
    padding: 10px;
    max-height: calc(100dvh - 64px);
    overflow: auto;
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

    .toolbar {
      flex-wrap: wrap;
      min-height: 48px;
      padding: 8px;
    }

    .toolbar strong {
      flex-basis: 100%;
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

    footer {
      flex-wrap: wrap;
      padding: 8px;
      gap: 8px;
    }

    .message {
      flex-basis: 100%;
      white-space: normal;
      overflow-wrap: anywhere;
    }

    .recovery-row {
      flex-wrap: wrap;
    }

    .diagnostics {
      position: fixed;
      top: 12px;
      max-height: calc(100dvh - 24px);
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

  .diagnostics label {
    display: flex;
    gap: 8px;
    margin-top: 10px;
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
