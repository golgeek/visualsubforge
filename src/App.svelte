<script lang="ts">
  import CueTable from './lib/CueTable.svelte';
  import CueEditor from './lib/CueEditor.svelte';
  import WaveformPanel from './lib/WaveformPanel.svelte';
  import { CueTableView } from './lib/cue-table-view.svelte';
  import type { WorkspaceControls } from './lib/workspace-controls';
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
    traceApplyInput,
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
    type MediaAction,
    type DocumentAction,
    type DocumentSnapshot,
    type WorkspaceAction,
  } from './lib/client';
  import { clamp, formatTime, parseTime } from './lib/timeline';
  import {
    windowsShortcut,
    editingShortcut,
    type EditingShortcut,
  } from './lib/shortcuts';
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
    if (alive) error = String(failure);
  });
  const timeline: TimelineView = new TimelineView({
    status: () => status,
    mediaPath: () => mediaPath,
    follow: () => follow,
    preview: () => timingEdit?.preview ?? null,
    gesture: () => waveformGesture,
    pending: () => mediaPending,
    cues: () => doc.cues,
    gap: () => timingSettings.cueGapMs,
    selectRange: (session, next) => liveLoop.update(session, next),
  });
  const gestures: TimingGesture = new TimingGesture(
    subtitles,
    media,
    timeline,
    {
      busy: () => busy,
      confirming: () => confirming,
      ready: () => ready,
      menu: () => cueMenu,
      cuts: () => sceneCuts,
      settings: () => timingSettings,
      operationContext: () => operationContext,
      command: docAct,
      run: (action) => run(action),
      suspendFollowing: suspendCueFollowing,
      revealSelection,
      focusWaveform,
      error: (message) => {
        error = message;
      },
      notice: (message) => {
        notice = message;
      },
    },
  );
  const tableView = new CueTableView({
    cues: () => doc.cues,
    position: () =>
      status.loaded && !mediaPending && !status.seeking
        ? status.positionSeconds
        : null,
  });
  const workspace: WorkspaceControls = {
    get busy() {
      return busy;
    },
    get confirming() {
      return confirming;
    },
    get ready() {
      return ready;
    },
    run,
    notice: (message) => {
      notice = message;
    },
  };
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
    if (busy || confirming || !ready) return;
    workspaceBusy = true;
    error = '';
    try {
      if (recover) {
        if (!(await protect())) return;
        await docAct({ kind: 'recover', id, discard: true });
        suspendCueFollowing();
        notice =
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
    } catch (e) {
      error = String(e);
    } finally {
      workspaceBusy = false;
    }
  }
  let textEditor = $state<HTMLTextAreaElement>();
  let waveHost = $state<HTMLDivElement>();
  const timingMode = $derived(gestures.mode);
  const heldTiming = $derived(gestures.held);
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
      await occludeVideo(diagnostics || confirming);
    } catch (e) {
      error = String(e);
    }
  }
  async function openCueMenu(
    x: number,
    y: number,
    cueId: number | null,
    inRange: boolean,
  ) {
    if (busy || confirming || !ready || (cueId === null && !inRange)) return;
    gestures.cancelHeld();
    const candidate = {
      x,
      y,
      cueId,
      range: inRange && range ? ([...range] as [number, number]) : null,
      generation: doc.generation,
      revision: doc.revision,
      session: status.session,
    };
    const request = ++menuEpoch;
    menuReturn =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    workspaceBusy = true;
    try {
      await occludeVideo(true);
      if (alive && request === menuEpoch) cueMenu = candidate;
    } catch (e) {
      error = String(e);
    } finally {
      workspaceBusy = false;
    }
  }
  async function chooseCueMenu(action: string) {
    const context = cueMenu;
    if (!context || busy) return;
    workspaceBusy = true;
    error = '';
    try {
      await closeCueMenu(false);
      if (
        context.generation !== doc.generation ||
        context.revision !== doc.revision ||
        context.session !== status.session
      )
        throw new Error('The document or media changed; open the menu again.');
      await applyDraft();
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
    } catch (e) {
      error = String(e);
    } finally {
      workspaceBusy = false;
    }
  }
  function cancelHeldTiming() {
    gestures.cancelHeld();
  }
  function releaseHeldTiming() {
    return gestures.releaseHeld();
  }
  async function editingKey(action: EditingShortcut) {
    if (busy || confirming || !ready || cueMenu || waveformGesture) return;
    cancelHeldTiming();
    if (action === 'play') {
      await run('play');
      return;
    }
    if (action === 'focusWaveform') {
      focusWaveform();
      return;
    }
    if (action === 'focusText') {
      await focusText();
      return;
    }
    if (action.startsWith('seek')) {
      if (status.positionSeconds !== null && status.loaded) {
        const amount = action.endsWith('Far') ? 5 : 1;
        await seek(
          clamp(
            status.positionSeconds +
              (action.includes('Back') ? -amount : amount),
            0,
            duration,
          ),
        );
      }
      return;
    }
    workspaceBusy = true;
    error = '';
    try {
      await applyDraft();
      const index = doc.cues.findIndex((c) => c.id === selected);
      const next =
        doc.cues[
          clamp(index + (action === 'nextCue' ? 1 : -1), 0, doc.cues.length - 1)
        ];
      if (next) {
        subtitles.select(next.id);
        await revealSelection();
        await focusText();
      }
    } catch (e) {
      error = String(e);
    } finally {
      workspaceBusy = false;
    }
  }
  let ready = $state(false);
  const workspaceModal = new WorkspaceModal({
    occlude: (value) => occludeVideo(value),
    remainingOcclusion: () => diagnostics,
    afterRender: tick,
    captureFocus: () => {
      const target = document.activeElement;
      return () => {
        if (target instanceof HTMLElement && target.isConnected) target.focus();
      };
    },
    reportError: (value) => {
      error = String(value);
    },
  });
  let workspaceBusy = $state(false);
  const busy = $derived(
    workspaceBusy || workspaceModal.opening || gestures.active,
  );
  let textFocused = $state(false);
  let follow = $state(true);
  let waveformGesture = $state(false);
  let cueFollowing = $state<CueFollowing>('following');
  let editorFocused = $state(false);
  let lastRevealedCue = '';
  const mediaPending = $derived(media.pending);
  const activeIds = $derived(tableView.activeIds);
  let error = $state('');
  let notice = $state('');
  let mediaPath = $state('');
  let restoredScenes = $state<{ session: number; generation: number } | null>(
    null,
  );
  let showScenes = $state(true);
  let preferences = $state<Preferences>({
    ...defaultPreferences,
    timing: { ...defaultTiming },
  });
  const statisticsOpen = $derived(workspaceModal.identity === 'statistics');
  const stylesOpen = $derived(workspaceModal.identity === 'styles');
  const fontsOpen = $derived(workspaceModal.identity === 'fonts');
  const conversionOpen = $derived(workspaceModal.identity === 'conversion');
  const pluginsOpen = $derived(workspaceModal.identity === 'plugins');
  let legacyImportOpen = $state(false);
  let legacyImportReturn: HTMLElement | null = null;
  const settingsOpen = $derived(workspaceModal.identity === 'settings');
  const resyncOpen = $derived(workspaceModal.identity === 'resync');
  const timingReview = $derived(
    workspaceModal.identity !== null &&
      typeof workspaceModal.identity === 'object'
      ? workspaceModal.identity.variant
      : null,
  );
  const timingOpen = $derived(timingReview !== null);
  const timingLabels: Record<TimingKind, string> = {
    gap: 'Gap',
    scene: 'Scene',
    duration: 'Duration and speed',
    batch: 'Batch',
  };
  const TimingDialog = $derived(
    timingReview === 'duration'
      ? DurationRepairDialog
      : timingReview === 'scene'
        ? SceneRepairDialog
        : GapDialog,
  );
  const textOpen = $derived(workspaceModal.identity === 'text');
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
    if (!alive || !settingsOpen) return;
    if (value) {
      preferences = { ...value, timing: { ...value.timing } };
      timingSettings = { ...value.timing };
      showQualityMetrics = value.showQualityMetrics;
      ++qualityContext;
      notice = 'Settings saved';
    }
    await workspaceModal.close();
  }

  async function closeResync(accepted = false) {
    if (!alive || !resyncOpen) return;
    if (accepted)
      notice =
        'Timings accepted. Use Save As to preserve the original subtitles.';
    await workspaceModal.close();
  }

  async function openStatistics() {
    if (
      busy ||
      confirming ||
      !ready ||
      !doc.loaded ||
      cueMenu ||
      waveformGesture
    )
      return;
    error = '';
    await workspaceModal.open('statistics');
  }

  async function openAuthoring(kind: AuthoringModal) {
    if (
      busy ||
      confirming ||
      !ready ||
      (kind === 'plugins'
        ? !doc.loaded
        : kind === 'conversion'
          ? !doc.loaded || doc.format?.name === 'vtt'
          : !subtitles.styled) ||
      waveformGesture ||
      cueMenu
    )
      return;
    error = '';
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
    if (outputPath) notice = `Converted subtitles exported to ${outputPath}`;
    if (accepted) {
      notice = `${kind} changes accepted. Undo restores the whole change.`;
      quality.restart();
    }
    await workspaceModal.close();
  }
  async function closeLegacyImport(outputPath?: string, open = false) {
    workspaceBusy = true;
    if (outputPath) notice = `Project created at ${outputPath}`;
    try {
      await legacyDecision.answer({ outputPath, open });
      const choice = await legacyDecision.wait();
      if (!alive || !choice) return;
      // Restoration completes before entering the existing protected open flow.
      if (choice.open && choice.outputPath)
        await projects.openPath(choice.outputPath);
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) {
        workspaceBusy = false;
        await tick();
        if (alive && legacyImportReturn?.isConnected)
          legacyImportReturn.focus();
      }
    }
  }
  async function closeStatistics(id?: string) {
    if (!alive || !statisticsOpen) return;
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
    if (
      busy ||
      confirming ||
      !ready ||
      !doc.cues.length ||
      cueMenu ||
      waveformGesture
    )
      return;
    error = '';
    suspendCueFollowing();
    await workspaceModal.open({ kind: 'timing', variant: kind });
  }
  async function openText(mode?: 'find' | 'replace' | TextOperation) {
    if (
      busy ||
      confirming ||
      !ready ||
      !doc.cues.length ||
      cueMenu ||
      waveformGesture
    )
      return;
    error = '';
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
    if (!alive || !textOpen) return;
    textOperation = operation;
    if (accepted) {
      notice = 'Text changes accepted. Undo restores the whole change.';
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
    if (draftDirty || busy) return;
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
    if (busy || confirming || !cue || !textEditor) return;
    const before = subtitles.text;
    const beforeStyle = subtitles.style;
    const snapshot = doc;
    const id = selected;
    const start = textEditor.selectionStart;
    const end = textEditor.selectionEnd;
    workspaceBusy = true;
    error = '';
    try {
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
      textEditor.focus();
      textEditor.setSelectionRange(result.start, result.end);
      notice = 'Formatting updated in the cue draft. Apply edit to accept it.';
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) workspaceBusy = false;
    }
  }
  async function closeTimingReview(accepted = false) {
    if (!alive || timingReview === null) return;
    if (accepted) {
      notice = `${timingLabels[timingReview]} timings accepted. Undo restores the whole change.`;
      quality.restart();
    }
    await workspaceModal.close();
  }

  let hardware = $state(true);
  const selected = $derived(subtitles.selected);
  const cue = $derived(subtitles.cue);
  const timeStepMs = $derived(doc.format?.timeStepMs ?? 1);
  const draftDirty = $derived(subtitles.dirty);
  const timingEdit = $derived(gestures.edit);
  const liveLoop = media.loop;
  function selectRange(next: [number, number]) {
    timeline.selectRange(next);
  }
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
  const editorMinimum = $derived(panes.editorMinimum);
  const tableMinimum = $derived(panes.tableMinimum);
  const paneHeight = $derived(panes.paneHeight);
  const topPixels = $derived(panes.topPixels);
  const editorPixels = $derived(panes.editorPixels);
  const start = $derived(timeline.start);
  const span = $derived(timeline.span);
  const range = $derived(timeline.range);
  const operationContext = $derived({
    timeStepMs,
    cues: doc.cues,
    selected,
    loaded: doc.loaded,
    draftDirty,
    positionSeconds:
      status.loaded && !status.seeking && !mediaPending
        ? status.positionSeconds
        : null,
    range,
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
  const wave = $derived(timeline.wave);
  let timingSettings = $state({ ...defaultTiming });
  const snapHint = $derived(gestures.snapHint);
  let sceneCuts = $state<number[]>([]);
  const waveError = $derived(timeline.error);
  let list = $state<HTMLDivElement>();
  const firstRow = $derived(tableView.firstRow);
  const visibleCues = $derived(tableView.visibleCues);
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
      enabled: ready && doc.loaded && (problemsOpen || showQualityMetrics),
      generation: doc.generation,
      revision: doc.revision,
      context: `${status.session}/${qualityContext}/${sceneState.qualityKey}`,
      selected,
      metricsOffset: firstRow,
      metricsLimit: showQualityMetrics ? visibleCues.length : 0,
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
  const duration = $derived(timeline.duration);
  let confirmation: HTMLDialogElement;
  function decisionEffects(close: () => void) {
    return {
      occlude: occludeVideo,
      remainingOcclusion: () => diagnostics,
      close,
      afterRender: tick,
      reportError: (failure: unknown) => {
        error = String(failure);
      },
    };
  }
  const subtitleDecision = new WorkspaceDecision<'save' | 'discard' | 'cancel'>(
    'cancel',
    decisionEffects(() => confirmation.close()),
  );
  const legacyDecision = new WorkspaceDecision<LegacyImportChoice | null>(
    null,
    decisionEffects(() => {
      legacyImportOpen = false;
    }),
  );
  let projectExitError = $state<string | null>(null);
  const projectExitDecision = new WorkspaceDecision<ProjectExitChoice>(
    'cancel',
    decisionEffects(() => {
      projectExitError = null;
    }),
  );
  let projectReferenceIssue = $state<ProjectReferenceIssue | null>(null);
  const projectReferenceDecision = new WorkspaceDecision<
    'cancel' | 'choose' | 'reload'
  >(
    'cancel',
    decisionEffects(() => {
      projectReferenceIssue = null;
    }),
  );
  const confirming = $derived(
    legacyDecision.visible ||
      legacyDecision.closing ||
      subtitleDecision.active ||
      projectExitDecision.active ||
      projectReferenceDecision.active ||
      workspaceModal.active,
  );
  let lastMenu = '';
  let alive = true;
  function docAct(action: DocumentAction, plan?: CueOperationPlan) {
    return subtitles.command(action, plan ? { plan } : {});
  }
  function applyDraft() {
    return subtitles.apply();
  }
  async function save(as: boolean) {
    const saved = await subtitles.save(as);
    if (saved) notice = 'Subtitles saved';
    return saved;
  }
  const projects = new ProjectWorkflow(subtitles, media, {
    read: () => ({
      mediaPath,
      hardware,
      options: {
        waveformStart: start,
        waveformSpan: span,
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
      notice = message;
    },
  });
  function occludeVideo(occluded: boolean) {
    return media.occlude(occluded);
  }
  function confirmChanges() {
    return subtitleDecision.ask(() => confirmation.showModal());
  }
  function answer(value: 'save' | 'discard' | 'cancel') {
    return subtitleDecision.answer(value);
  }
  async function protect(saveContext?: () => Promise<boolean>) {
    if (!alive) return false;
    if (!doc.dirty && !draftDirty && !doc.translation?.dirty) return true;
    const choice = await confirmChanges();
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
    if (busy || confirming || !ready) return;
    workspaceBusy = true;
    error = '';
    try {
      if (operation === 'attach' || operation === 'copy') {
        const path = await chooseFile(
          'subtitles',
          doc.translation?.reference?.path ?? doc.path,
        );
        if (!alive || !path) return;
        if (operation === 'copy' && !(await protect())) return;
        if (!alive) return;
        if (await translation.load(path, operation === 'copy')) {
          notice =
            operation === 'copy'
              ? 'Independent translation target created · Save As required'
              : 'Reference attached';
          ++qualityContext;
        }
      } else if (operation === 'offset') await translation.setOffset(offsetMs);
      else await translation.detach();
    } catch (failure) {
      if (alive) error = String(failure);
    } finally {
      if (alive) workspaceBusy = false;
    }
  }
  $effect(() => {
    if (!translationOpen) translation.request(doc, null, 0, null, false);
  });
  function resolveProjectReference(issue: ProjectReferenceIssue) {
    return projectReferenceDecision.ask(() => {
      projectReferenceIssue = issue;
    });
  }
  function answerProjectReference(choice: 'cancel' | 'choose' | 'reload') {
    return projectReferenceDecision.answer(choice);
  }
  function projectSaveFailed(failure: unknown): Promise<ProjectExitChoice> {
    return projectExitDecision.ask(() => {
      projectExitError = String(failure);
    });
  }
  function answerProjectExit(choice: ProjectExitChoice) {
    return projectExitDecision.answer(choice);
  }
  function act(action: MediaAction) {
    cancelHeldTiming();
    return media.command(action);
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
      busy ||
      cueMenu ||
      (expected && doc !== expected) ||
      !doc.cues.some((c) => c.id === id)
    )
      return;
    suspendCueFollowing();
    cancelHeldTiming();
    workspaceBusy = true;
    error = '';
    try {
      await applyDraft();
      if (!alive || (expected && doc !== expected)) return;
      subtitles.select(id, mode);
      const selectedCue = subtitles.cue;
      if (selectedCue)
        selectRange([selectedCue.startMs / 1000, selectedCue.endMs / 1000]);
      await revealSelection();
      if (!alive || (expected && doc !== expected)) return;
      if (cue && status.loaded)
        await act({ kind: 'seek', seconds: cue.startMs / 1000 });
    } catch (e) {
      error = String(e);
    } finally {
      workspaceBusy = false;
    }
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
      if (next && !busy && !confirming)
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
    if (busy || confirming || cueMenu) return;
    workspaceBusy = true;
    error = '';
    suspendCueFollowing();
    try {
      await applyDraft();
      subtitles.selectAll();
    } catch (e) {
      error = String(e);
    } finally {
      workspaceBusy = false;
    }
  }
  function requestProblem(id: string) {
    if (
      busy ||
      confirming ||
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
    if (busy || !pendingProblem) return;
    const id = pendingProblem;
    if (choice === 'cancel') {
      pendingProblem = null;
      return;
    }
    workspaceBusy = true;
    error = '';
    try {
      if (choice === 'apply') await applyDraft();
      else subtitles.revert();
      pendingProblem = null;
    } catch (failure) {
      error = String(failure);
      return;
    } finally {
      workspaceBusy = false;
    }
    if (alive) await selectCue(Number(id));
  }
  async function run(action: WorkspaceAction, event?: Event) {
    if (busy || confirming || !ready || cueMenu || waveformGesture) return;
    if (action === 'plugins') {
      cancelHeldTiming();
      await openAuthoring('plugins');
      return;
    }
    if (action === 'convertSubtitles') {
      cancelHeldTiming();
      await openAuthoring('conversion');
      return;
    }
    if (action === 'showScenes') {
      showScenes = !showScenes;
      return;
    }
    if (action === 'scanScenes') {
      await sceneControls?.rescan();
      return;
    }
    if (action === 'cancelSceneScan') {
      await sceneControls?.cancel();
      return;
    }
    if (action === 'previousScene' || action === 'nextScene') {
      sceneControls?.navigate(action === 'previousScene');
      return;
    }
    cancelHeldTiming();
    if (action === 'apply' && event && cue && draftDirty)
      traceApplyInput(event, doc);
    // Menu/keyboard actions respect the text field's local undo stack. Explicit
    // history buttons use document history even if the platform retains focus
    // in the editor while dispatching their click.
    const focused = document.activeElement;
    if (
      !event &&
      (action === 'undo' || action === 'redo') &&
      (focused instanceof HTMLInputElement ||
        focused instanceof HTMLTextAreaElement)
    ) {
      document.execCommand(action);
      return;
    }
    workspaceBusy = true;
    error = '';
    notice = '';
    try {
      switch (action) {
        case 'resync':
          if (!status.loaded || !doc.cues.length) break;
          await workspaceModal.open('resync');
          break;
        case 'settings':
        case 'timingSettings':
          await workspaceModal.open('settings');
          break;
        case 'openProject':
          await projects.open();
          break;
        case 'translationReference':
          translationOpen = !translationOpen;
          if (translationOpen) problemsOpen = false;
          break;
        case 'importLegacyProject':
          legacyImportReturn =
            document.activeElement instanceof HTMLElement
              ? document.activeElement
              : null;
          await legacyDecision.open(() => {
            legacyImportOpen = true;
          });
          break;
        case 'saveProject':
          await projects.save(false);
          break;
        case 'saveProjectAs':
          await projects.save(true);
          break;
        case 'createCue':
        case 'deleteCue':
        case 'splitCue':
        case 'mergePrevious':
        case 'mergeNext': {
          if (action === 'createCue') await applyDraft();
          const plan = planCueOperation(action, operationContext);
          suspendCueFollowing();
          await docAct(plan.action, plan);
          await revealSelection();
          if (action === 'createCue') await focusText();
          notice = `${cueOperationLabels[action]} applied`;
          break;
        }
        case 'openMedia': {
          const path = await chooseFile('media', mediaPath || null);
          if (path) {
            timeline.reset(true);
            await act({ kind: 'open', path, hardware });
            mediaPath = path;
            await projects.detach();
            restoredScenes = null;
            timingSettings = { ...preferences.timing };
          }
          break;
        }
        case 'closeMedia':
          await act({ kind: 'close' });
          mediaPath = '';
          await projects.detach();
          timingSettings = { ...preferences.timing };
          restoredScenes = null;
          timeline.reset();
          break;
        case 'openSubtitles': {
          const path = await chooseFile('subtitles', doc.path);
          if (path && (await protect())) {
            await docAct({ kind: 'open', path, discard: true });
            tableView.scrollTop = 0;
            if (list) list.scrollTop = 0;
          }
          break;
        }
        case 'closeSubtitles':
          if (await protect()) await docAct({ kind: 'close', discard: true });
          break;
        case 'apply':
          await applyDraft();
          notice = 'Edit applied';
          break;
        case 'save':
          if (doc.loaded) await save(false);
          break;
        case 'saveAs':
          if (doc.loaded) await save(true);
          break;
        case 'undo':
        case 'redo':
          if (draftDirty)
            throw new Error(
              'Apply or revert the current draft before undoing an accepted edit.',
            );
          if (doc.loaded) {
            suspendCueFollowing();
            await docAct({ kind: action });
            await revealSelection();
          }
          break;
        case 'play':
          if (status.loaded)
            await act({ kind: 'pause', paused: !status.paused });
          break;
        case 'previousFrame':
        case 'nextFrame':
          if (status.loaded)
            await act({ kind: 'step', backwards: action === 'previousFrame' });
          break;
        case 'playRange':
          if (status.loaded && range && range[1] > range[0]) {
            await act({ kind: 'loop', start: range[0], end: range[1] });
            await act({ kind: 'seek', seconds: range[0] });
            await act({ kind: 'pause', paused: false });
          }
          break;
        case 'clearLoop':
          if (status.loaded) await act({ kind: 'clearLoop' });
          break;
        case 'resetLayout':
          topPct = 32;
          editorPct = 25;
          wavePct = 65;
          break;
        case 'diagnostics':
          if (!diagnostics) {
            await occludeVideo(true);
            diagnostics = true;
          } else {
            diagnostics = false;
            await tick();
            await occludeVideo(false);
          }
          break;
        case 'quit':
          await projects.quit();
          break;
      }
    } catch (e) {
      error = String(e);
    } finally {
      workspaceBusy = false;
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
    if (!status.loaded || mediaPending || busy) return;
    try {
      await act({ kind: 'seek', seconds: time });
    } catch (e) {
      error = String(e);
    }
  }
  async function selectTrack(id: number) {
    if (busy) return;
    workspaceBusy = true;
    try {
      await act({ kind: 'track', id });
      timeline.invalidateTrack();
    } catch (e) {
      error = String(e);
    } finally {
      workspaceBusy = false;
    }
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
      range: !!range && range[1] > range[0],
      busy: busy || confirming || !ready || waveformGesture,
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
    void updateMenu(state).catch((e) => (error = String(e)));
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
      draftDirty ||
      editorFocused ||
      busy ||
      confirming ||
      !ready ||
      mediaPending;
    const next = followedSelection(
      activeIds,
      selected,
      cueFollowing,
      protectedEdit,
    );
    if (cueFollowing !== 'following') lastRevealedCue = '';
    if (
      protectedEdit ||
      cueFollowing !== 'following' ||
      next === null ||
      !activeIds.includes(next)
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
        if (alive) error = String(e);
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
        if (alive) error = String(e);
      })
      .finally(() => {
        if (alive) ready = true;
      });
    void workspaceActions((action) => void run(action))
      .then((stop) => {
        if (alive) unlisten = stop;
        else stop();
      })
      .catch((e) => (error = String(e)));
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
        if (alive) error = String(e);
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
  function workspaceKeydown(e: KeyboardEvent) {
    const editing =
      e.target instanceof HTMLInputElement ||
      e.target instanceof HTMLTextAreaElement ||
      (e.target instanceof HTMLElement && e.target.isContentEditable);
    if (e.isComposing || e.defaultPrevented || cueMenu || confirming) return;
    if (
      (e.metaKey || e.ctrlKey) &&
      !e.altKey &&
      !e.repeat &&
      ((e.key.toLowerCase() === 'f' && !e.shiftKey) ||
        (e.key.toLowerCase() === 'h' &&
          ((e.ctrlKey && !e.shiftKey) || (e.metaKey && e.shiftKey))))
    ) {
      e.preventDefault();
      void openText(e.key.toLowerCase() === 'f' ? 'find' : 'replace');
      return;
    }
    if (
      problemsOpen &&
      e.key === 'F4' &&
      !e.metaKey &&
      !e.ctrlKey &&
      !e.altKey
    ) {
      e.preventDefault();
      const id = e.shiftKey ? quality.view?.previousCue : quality.view?.nextCue;
      if (id && !quality.stale) requestProblem(id);
      return;
    }
    const shortcut = editingShortcut(e, editing);
    // Enter should activate a focused button/select rather than move focus.
    if (
      shortcut &&
      !(
        shortcut === 'focusText' &&
        (e.target instanceof HTMLButtonElement ||
          e.target instanceof HTMLSelectElement)
      )
    ) {
      e.preventDefault();
      void editingKey(shortcut);
      return;
    }
    if (__WINDOWS__) {
      const action = windowsShortcut(e, editing);
      if (action) {
        e.preventDefault();
        if (action !== 'openMedia' || status.available) void run(action);
        return;
      }
    }
    if (
      e.code === 'Space' &&
      !editing &&
      !e.ctrlKey &&
      !e.metaKey &&
      !e.altKey &&
      !e.shiftKey &&
      !(e.target instanceof HTMLButtonElement) &&
      !(e.target instanceof HTMLSelectElement)
    ) {
      e.preventDefault();
      if (e.repeat || busy || confirming || !ready) return;
      if (timingMode && !status.paused) {
        gestures.beginHeld();
      } else void run('play');
    }
  }
</script>

<svelte:window
  onblur={cancelHeldTiming}
  onkeyup={(e) => {
    if (e.code === 'Space' && heldTiming) {
      e.preventDefault();
      void releaseHeldTiming();
    }
  }}
  onfocusin={(e) =>
    (textFocused =
      e.target instanceof HTMLInputElement ||
      e.target instanceof HTMLTextAreaElement)}
  onfocusout={() => (textFocused = false)}
  onkeydown={workspaceKeydown}
/>
<main
  aria-busy={busy || !ready}
  class:has-recovery={recovery.entries.length > 0 || !!recovery.error}
  style:--workspace-height={`${workspaceHeight}px`}
  style:--workspace-min-height={`${148 + tableMinimum + editorMinimum}px`}
  style:--table-min-height={`${tableMinimum}px`}
>
  <header class="toolbar">
    <strong>VisualSubForge</strong>
    {#if !nativeMenus}
      <button disabled={busy || !ready} onclick={() => void run('openProject')}
        >Open Project</button
      >
      <button
        disabled={busy || confirming || !ready}
        onclick={() => void run('importLegacyProject')}
        >Import VisualSubSync Project</button
      >
      <button
        disabled={busy || !ready || (!status.loaded && !doc.loaded)}
        title={projects.path ??
          'Save subtitle references, media, scene results and workspace options'}
        onclick={() => void run('saveProject')}>Save Project</button
      >
      <button
        disabled={busy || !ready || !status.available}
        onclick={() => void run('openMedia')}>Open Media</button
      >
      <button
        disabled={busy || !ready}
        onclick={() => void run('openSubtitles')}>Open Subtitles</button
      >
      <button disabled={busy || !doc.loaded} onclick={() => void run('save')}
        >Save</button
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
      <button
        disabled={busy || confirming || !ready}
        onclick={() => void run('settings')}>Settings…</button
      >
      <button
        disabled={busy ||
          confirming ||
          !ready ||
          !status.loaded ||
          !doc.cues.length}
        onclick={() => void run('resync')}>Resynchronize subtitles…</button
      >
      <button
        disabled={!status.loaded || busy || confirming || sceneState.pending}
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
            disabled={busy || !ready || !!entry.error}
            onclick={() => void chooseRecovery(entry.id, true)}>Recover</button
          >
          {#if discardRecoveryId === entry.id}
            <span>Discard this recovery snapshot?</span>
            <button
              disabled={busy}
              onclick={() => void chooseRecovery(entry.id, false)}
              >Confirm discard</button
            >
            <button disabled={busy} onclick={() => (discardRecoveryId = null)}
              >Cancel</button
            >
          {:else}
            <button
              disabled={busy}
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
    style:--top-height={`${topPixels}px`}
    style:--editor-height={`${editorPixels}px`}
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
      value={(topPixels / paneHeight) * 100}
      onchange={(v) => (topPct = clamp(v, 20, Math.min(55, 80 - editorPct)))}
    />
    <section class="subtitle-panel panel" aria-label="Subtitle table">
      <div
        class="transport"
        use:measurePaneControl={(height) => (transportHeight = height)}
      >
        <button
          disabled={!status.loaded || busy}
          onclick={() => void run('play')}
          >{status.paused ? 'Play' : 'Pause'}</button
        ><button
          disabled={!status.loaded || busy}
          onclick={() => void run('previousFrame')}
          aria-label="Previous frame">◀|</button
        ><button
          disabled={!status.loaded || busy}
          onclick={() => void run('nextFrame')}
          aria-label="Next frame">|▶</button
        ><output
          >{status.positionSeconds === null
            ? '—'
            : formatTime(status.positionSeconds * 1000)}</output
        ><label
          >Audio <select
            value={status.audioTrack ?? ''}
            disabled={!status.loaded || busy}
            onchange={(e) => void selectTrack(Number(e.currentTarget.value))}
            >{#each status.audioTracks as track (track.id)}<option
                value={track.id}>{track.title}</option
              >{/each}</select
          ></label
        ><span class="spacer"></span><button
          disabled={!range || !status.loaded || busy}
          onclick={() => void run('playRange')}>Loop range</button
        ><button
          disabled={!status.loaded || busy}
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
            disabled={!op.plan || busy || confirming}
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
          disabled={busy || confirming || !doc.cues.length}
          onclick={() => void openText()}>Find / Replace…</button
        >
        <button
          disabled={busy || confirming || !doc.cues.length}
          onclick={() => void openTimingReview('gap')}
          >Enforce minimum gaps…</button
        >
        <button
          disabled={busy || confirming || !doc.cues.length}
          onclick={() => void openTimingReview('scene')}
          >Repair scene timing…</button
        >
        <button
          disabled={busy || confirming || !doc.cues.length}
          onclick={() => void openTimingReview('duration')}
          >Repair duration / speed…</button
        >
        <button
          disabled={busy || confirming || !doc.cues.length}
          onclick={() => void openTimingReview('batch')}
          >Shift / align cues…</button
        >

        <button
          disabled={busy || confirming || !doc.loaded}
          onclick={() => void openStatistics()}>Statistics…</button
        >
        <button
          disabled={busy || confirming || !doc.loaded}
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
          disabled={busy || confirming || !ready}
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
            checked={timingMode}
            disabled={busy || confirming}
            onchange={(e) => {
              cancelHeldTiming();
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
                draftDirty || editorFocused || busy,
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
            disabled={draftDirty || busy || confirming}
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
            disabled={busy || confirming}
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
            disabled={busy || confirming || !ready}
            onload={(copy) => void referenceOperation(copy ? 'copy' : 'attach')}
            onoffset={(offset) => void referenceOperation('offset', offset)}
            ondetach={() => void referenceOperation('detach')}
            onclose={() => (translationOpen = false)}
          />{/if}
      </div>
    </section>
    <Splitter
      label="Resize subtitle list and editor"
      value={100 - (editorPixels / paneHeight) * 100}
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
      title={error || status.error || status.subtitlePreviewError || undefined}
      >{error ||
        status.error ||
        status.subtitlePreviewError ||
        notice ||
        (busy ? 'Working…' : 'Ready')}</span
    >{#if timingEdit}<span
        >{snapHint || 'Timing preview'}
        {formatTime(timingEdit.preview.startMs)}–{formatTime(
          timingEdit.preview.endMs,
        )}</span
      >{/if}{#if range}<span
        >Range {range[0].toFixed(3)}–{range[1].toFixed(3)} s</span
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
        Waveform stream {wave?.audioStreamIndex ?? '—'} · container origin {wave?.containerOriginSeconds ??
          '—'} s
      </p>
      <p>{waveError || wave?.error || ''}</p>
    </aside>{/if}
  <dialog
    bind:this={confirmation}
    oncancel={(e) => {
      e.preventDefault();
      answer('cancel');
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
      <button onclick={() => answer('cancel')}>Cancel</button><button
        onclick={() => answer('discard')}>Discard changes</button
      ><button onclick={() => answer('save')}
        >{doc.translation?.dirty ? 'Save project' : 'Save changes'}</button
      >
    </div>
  </dialog>
  {#if projectExitError !== null}
    <ProjectSaveFailureDialog
      error={projectExitError}
      onchoose={(choice) => void answerProjectExit(choice)}
    />
  {/if}
  {#if projectReferenceIssue !== null}
    <ProjectReferenceDialog
      issue={projectReferenceIssue}
      onchoose={(choice) => void answerProjectReference(choice)}
    />
  {/if}
  {#if resyncOpen}
    <ResyncDialog
      {subtitles}
      media={() => status}
      target={mediaPath}
      onclose={() => void closeResync()}
      onaccepted={() => void closeResync(true)}
    />
  {/if}
  {#if statisticsOpen}
    <StatisticsDialog
      {subtitles}
      mediaSession={() => status.session}
      context={() => `${qualityContext}/${sceneState.qualityKey}`}
      onclose={() => void closeStatistics()}
      onnavigate={(id) => void closeStatistics(id)}
    />
  {/if}
  {#if fontsOpen && doc.format && subtitles.styled}
    <FontDialog
      {subtitles}
      onclose={() => void closeAuthoring()}
      onaccepted={() => void closeAuthoring(true)}
    />
  {/if}
  {#if pluginsOpen}
    <PluginDialog
      {subtitles}
      media={() => status}
      context={() => `${qualityContext}/${sceneState.qualityKey}`}
      onclose={() => void closeAuthoring()}
      onaccepted={() => void closeAuthoring(true)}
    />
  {/if}
  {#if conversionOpen}
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
  {#if stylesOpen && doc.format && subtitles.styled}
    <StyleInspector
      {subtitles}
      initial={subtitles.style}
      onclose={() => void closeAuthoring()}
      onaccepted={() => void closeAuthoring(true)}
    />
  {/if}
  {#if timingOpen}
    {#if timingReview === 'batch'}
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
  {#if textOpen}
    <TextDialog
      {subtitles}
      initial={textOperation}
      onclose={(operation) => void closeText(operation)}
      onaccepted={(operation) => void closeText(operation, true)}
      onnavigate={(id, start, end, operation) =>
        void navigateText(id, start, end, operation)}
    />
  {/if}
  {#if settingsOpen}
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
