<script lang="ts">
  import SettingsDialog from './lib/SettingsDialog.svelte';
  import ResyncDialog from './lib/ResyncDialog.svelte';
  import ProblemsPanel from './lib/ProblemsPanel.svelte';
  import StatisticsDialog from './lib/StatisticsDialog.svelte';
  import StyleInspector from './lib/StyleInspector.svelte';
  import CueMetrics from './lib/CueMetrics.svelte';
  import GapDialog from './lib/GapDialog.svelte';
  import SceneRepairDialog from './lib/SceneRepairDialog.svelte';
  import DurationRepairDialog from './lib/DurationRepairDialog.svelte';
  import TextDialog from './lib/TextDialog.svelte';
  import { QualitySession } from './lib/quality-session.svelte';
  import { rate } from './lib/quality-display';
  import { isTauri } from '@tauri-apps/api/core';
  import {
    defaultTiming,
    snapBoundary,
    type TimingSettings,
  } from './lib/magnetic-timing';
  import {
    defaultPreferences,
    getPreferences,
    type Preferences,
  } from './lib/client';
  import { DocumentSession } from './lib/document-session.svelte';
  import { MediaSession } from './lib/media-session.svelte';
  import { ProjectWorkflow } from './lib/project-workflow.svelte';
  import { PendingDecision } from './lib/pending-decision';
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
    waveformStart,
    waveformView,
    type MediaAction,
    type DocumentAction,
    type WorkspaceAction,
    type WaveformView,
    type Cue,
  } from './lib/client';
  import { clamp, formatTime, parseTime, viewport } from './lib/timeline';
  import {
    playbackViewportStart,
    waveformRequestWindow,
  } from './lib/waveform-navigation';
  import {
    windowsShortcut,
    editingShortcut,
    type EditingShortcut,
  } from './lib/shortcuts';
  import CueContextMenu from './lib/CueContextMenu.svelte';
  import {
    beginHeldTiming,
    advanceHeldTiming,
    resolveTimingRelease,
    type HeldTiming,
  } from './lib/hold-timing';
  import {
    cueOperationLabels,
    planCueOperation,
    type CueOperation,
    type CueOperationPlan,
  } from './lib/cue-operations';
  import {
    activeCueIds,
    changeFollowing,
    followedSelection,
    revealCueRow,
    type CueFollowing,
  } from './lib/cue-following';
  import Splitter from './lib/Splitter.svelte';
  import Waveform from './lib/Waveform.svelte';
  import SceneControls from './lib/SceneControls.svelte';
  import ProjectSaveFailureDialog, {
    type ProjectExitChoice,
  } from './lib/ProjectSaveFailureDialog.svelte';
  import CueTiming from './lib/CueTiming.svelte';
  import {
    beginTimingEdit,
    previewBoundary,
    timingAction,
    type TimingEdit,
    type CueBoundary,
  } from './lib/cue-timing';
  const subtitles = new DocumentSession();
  const quality = new QualitySession();
  let problemsOpen = $state(false);
  let showQualityMetrics = $state(false);
  let qualityContext = $state(0);
  let pendingProblem = $state<string | null>(null);
  let pendingProblemVersion = '';
  const media = new MediaSession((failure) => {
    if (alive) error = String(failure);
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
    if (busy || confirming || !ready) return;
    busy = true;
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
      busy = false;
    }
  }
  let textEditor: HTMLTextAreaElement;
  let waveHost: HTMLDivElement;
  let timingMode = $state(false);
  let heldTiming = $state<HeldTiming | null>(null);
  let timingPressedAt = 0;
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
    heldTiming = null;
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
    busy = true;
    try {
      await occludeVideo(true);
      if (alive && request === menuEpoch) cueMenu = candidate;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  async function chooseCueMenu(action: string) {
    const context = cueMenu;
    if (!context || busy) return;
    busy = true;
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
      busy = false;
    }
  }
  function cancelHeldTiming() {
    heldTiming = null;
  }
  $effect(() => {
    if (!heldTiming) return;
    const next =
      timingMode && !mediaPending
        ? advanceHeldTiming(heldTiming, doc, status)
        : null;
    if (!next || next.last !== heldTiming.last) heldTiming = next;
  });
  async function releaseHeldTiming() {
    const held = heldTiming;
    heldTiming = null;
    if (!held || busy || confirming || cueMenu) return;
    const result = resolveTimingRelease(
      held,
      doc,
      status,
      performance.now() - timingPressedAt,
    );
    if (!result) {
      notice = 'Timing cancelled: playback changed or the interval was empty.';
      return;
    }
    if (result.kind === 'pause') {
      await run('play');
      return;
    }
    const marked = result.range;
    busy = true;
    error = '';
    try {
      await applyDraft();
      const plan = planCueOperation('createCue', {
        ...operationContext,
        range: marked,
        draftDirty: false,
      });
      suspendCueFollowing();
      await docAct(plan.action, plan);
      range = marked;
      await revealSelection();
      focusWaveform();
      notice = 'Cue created · hold Space for the next cue; Enter edits text.';
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
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
    busy = true;
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
      busy = false;
    }
  }
  let ready = $state(false);
  let busy = $state(false);
  let textFocused = $state(false);
  let follow = $state(true);
  let waveformGesture = $state(false);
  let cueFollowing = $state<CueFollowing>('following');
  let editorFocused = $state(false);
  let lastRevealedCue = '';
  const mediaPending = $derived(media.pending);
  const activeIds = $derived(
    activeCueIds(
      doc.cues,
      status.loaded && !mediaPending && !status.seeking
        ? status.positionSeconds
        : null,
    ),
  );
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
  let statisticsOpen = $state(false);
  let stylesOpen = $state(false);
  let stylesReturn: HTMLElement | null = null;
  let statisticsReturn: HTMLElement | null = null;
  let settingsOpen = $state(false);
  let resyncOpen = $state(false);
  let timingOpen = $state(false);
  let timingReview = $state<'gap' | 'scene' | 'duration'>('gap');
  const TimingDialog = $derived(
    timingReview === 'duration'
      ? DurationRepairDialog
      : timingReview === 'scene'
        ? SceneRepairDialog
        : GapDialog,
  );
  let timingReturn: HTMLElement | null = null;
  let textOpen = $state(false);
  let textReturn: HTMLElement | null = null;
  let textOperation = $state<TextOperation>({
    kind: 'find',
    query: '',
    caseSensitive: false,
  });
  let resyncReturn: HTMLElement | null = null;
  let settingsReturn: HTMLElement | null = null;
  const nativeMenus = isTauri();
  let sceneControls: {
    rescan: () => Promise<void>;
    cancel: () => Promise<void>;
    navigate: (backwards: boolean) => void;
  };
  let sceneState = $state({
    qualityKey: '',
    running: false,
    pending: false,
    previous: false,
    next: false,
  });
  async function closeSettings(value?: Preferences) {
    if (value) {
      preferences = { ...value, timing: { ...value.timing } };
      timingSettings = { ...value.timing };
      showQualityMetrics = value.showQualityMetrics;
      ++qualityContext;
      notice = 'Settings saved';
    }
    settingsOpen = false;
    try {
      await tick();
      await occludeVideo(diagnostics);
      if (settingsReturn?.isConnected) settingsReturn.focus();
    } catch (e) {
      error = String(e);
    } finally {
      confirming = false;
    }
  }

  async function closeResync(accepted = false) {
    resyncOpen = false;
    if (accepted)
      notice =
        'Timings accepted. Use Save As to preserve the original subtitles.';
    try {
      await tick();
      if (!alive) return;
      await occludeVideo(diagnostics);
      if (resyncReturn?.isConnected) resyncReturn.focus();
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) confirming = false;
    }
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
    busy = true;
    error = '';
    statisticsReturn =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    try {
      await occludeVideo(true);
      if (alive) {
        confirming = true;
        statisticsOpen = true;
      }
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) busy = false;
    }
  }
  async function openStyles() {
    if (
      busy ||
      confirming ||
      !ready ||
      !doc.format ||
      waveformGesture ||
      cueMenu
    )
      return;
    busy = true;
    error = '';
    stylesReturn =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    try {
      await occludeVideo(true);
      if (alive) {
        confirming = true;
        stylesOpen = true;
      }
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) busy = false;
    }
  }
  async function closeStyles() {
    stylesOpen = false;
    try {
      await tick();
      if (!alive) return;
      await occludeVideo(diagnostics);
      if (stylesReturn?.isConnected) stylesReturn.focus();
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) confirming = false;
    }
  }
  async function closeStatistics(id?: string) {
    const version = `${doc.generation}/${doc.revision}/${status.session}/${qualityContext}/${sceneState.qualityKey}`;
    statisticsOpen = false;
    try {
      await tick();
      if (!alive) return;
      await occludeVideo(diagnostics);
      if (statisticsReturn?.isConnected) statisticsReturn.focus();
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) confirming = false;
    }
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
  async function openTimingReview(kind: 'gap' | 'scene' | 'duration') {
    if (
      busy ||
      confirming ||
      !ready ||
      !doc.cues.length ||
      cueMenu ||
      waveformGesture
    )
      return;
    busy = true;
    error = '';
    timingReturn =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    try {
      await occludeVideo(true);
      if (alive) {
        confirming = true;
        timingReview = kind;
        timingOpen = true;
      }
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) busy = false;
    }
  }
  async function openText(mode?: 'find' | 'replace') {
    if (
      busy ||
      confirming ||
      !ready ||
      !doc.cues.length ||
      cueMenu ||
      waveformGesture
    )
      return;
    busy = true;
    error = '';
    textReturn =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    if (mode) {
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
    }
    try {
      await occludeVideo(true);
      if (alive) {
        confirming = true;
        textOpen = true;
      }
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) busy = false;
    }
  }
  async function closeText(operation: TextOperation, accepted = false) {
    textOperation = operation;
    textOpen = false;
    if (accepted) {
      notice = 'Text changes accepted. Undo restores the whole change.';
      quality.restart();
    }
    try {
      await tick();
      if (!alive) return;
      await occludeVideo(diagnostics);
      if (textReturn?.isConnected) textReturn.focus();
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) confirming = false;
    }
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
    await closeText(operation);
    if (!alive || draftDirty) return;
    await selectCue(id);
    await tick();
    if (selected === id && textEditor) {
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
    busy = true;
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
      if (alive) busy = false;
    }
  }
  async function closeTimingReview(accepted = false) {
    timingOpen = false;
    if (accepted) {
      notice = `${timingReview === 'duration' ? 'Duration and speed' : timingReview === 'scene' ? 'Scene' : 'Gap'} timings accepted. Undo restores the whole change.`;
      quality.restart();
    }
    try {
      await tick();
      if (!alive) return;
      await occludeVideo(diagnostics);
      if (timingReturn?.isConnected) timingReturn.focus();
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) confirming = false;
    }
  }

  let hardware = $state(true);
  const selected = $derived(subtitles.selected);
  const cue = $derived(subtitles.cue);
  const timeStepMs = $derived(doc.format?.timeStepMs ?? 1);
  const draftDirty = $derived(subtitles.dirty);
  let timingEdit = $state<TimingEdit | null>(null);
  let timingMagnets: {
    cues: Cue[];
    cuts: number[];
    settings: TimingSettings;
  } | null = null;
  let timingRangeBefore: [number, number] | null = null;
  const liveLoop = media.loop;
  function selectRange(next: [number, number]) {
    range = next;
    liveLoop.update(status.session, next);
  }
  const timingCue = $derived(timingEdit?.preview ?? cue);
  function beginTiming(): boolean {
    cancelHeldTiming();
    if (busy || confirming || !ready || !status.loaded || mediaPending)
      return false;
    try {
      timingEdit = beginTimingEdit(doc, selected, status.session, draftDirty);
      timingRangeBefore = range;
      timingMagnets = {
        cues: doc.cues,
        cuts: [...sceneCuts],
        settings: { ...timingSettings },
      };
      busy = true;
      suspendCueFollowing();
      error = '';
      notice = 'Timing preview · release to apply, Escape to cancel';
      return true;
    } catch (e) {
      error = String(e);
      return false;
    }
  }
  function previewTiming(
    boundary: CueBoundary,
    milliseconds: number,
    pointer?: { bypass: boolean; millisecondsPerPixel: number },
  ) {
    if (timingEdit && timingMagnets) {
      const result = snapBoundary(
        milliseconds,
        boundary,
        boundary === 'start'
          ? timingEdit.preview.endMs
          : timingEdit.preview.startMs,
        timingEdit.original.id,
        timingMagnets.cues,
        timingMagnets.cuts,
        timingMagnets.settings,
        (pointer?.millisecondsPerPixel ?? 0) *
          timingMagnets.settings.snapDistancePx,
        Math.floor(duration * 1000),
        !pointer || pointer.bypass,
      );
      snapHint = result.target ? `Snapped to ${result.target}` : '';
      timingEdit = previewBoundary(timingEdit, boundary, result.milliseconds);
      selectRange([
        timingEdit.preview.startMs / 1000,
        timingEdit.preview.endMs / 1000,
      ]);
    }
  }
  async function finishTiming(commit: boolean) {
    const edit = timingEdit;
    if (!edit) return;
    let accepted = false;
    try {
      const action = commit
        ? timingAction(edit, doc, status.session, selected)
        : null;
      if (action) {
        // Send the gesture's captured version, never substitute the latest one.
        await subtitles.command(action, {
          version: edit,
          preferred: edit.original.id,
        });
      }
      accepted = commit;
      notice = commit ? 'Timing applied' : 'Timing cancelled';
    } catch (e) {
      error = String(e);
    } finally {
      const finalCue = accepted ? edit.preview : edit.original;
      range = accepted
        ? [finalCue.startMs / 1000, finalCue.endMs / 1000]
        : timingRangeBefore;
      liveLoop.update(
        edit.mediaSession,
        range ?? [finalCue.startMs / 1000, finalCue.endMs / 1000],
      );
      await liveLoop.flush();
      timingEdit = null;
      timingMagnets = null;
      snapHint = '';
      busy = false;
    }
  }
  let diagnostics = $state(false);
  let video: HTMLDivElement;
  let workspaceHeight = $state(600);
  let topPct = $state(32);
  let editorPct = $state(25);
  let wavePct = $state(65);
  const editorMinimum = $derived(showQualityMetrics ? 190 : 130);
  const paneHeight = $derived(
    Math.max(352 + editorMinimum, workspaceHeight - 16),
  );
  // Reserve room for the table controls and visible rows when either divider
  // grows a neighboring pane. The editor and preview retain usable minima.
  const topPixels = $derived(
    clamp((paneHeight * topPct) / 100, 120, paneHeight - 232 - editorMinimum),
  );
  const editorPixels = $derived(
    clamp(
      (paneHeight * editorPct) / 100,
      editorMinimum,
      paneHeight - topPixels - 232,
    ),
  );
  let start = $state(0);
  let span = $state(20);
  let range = $state<[number, number] | null>(null);
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
  let wave = $state<WaveformView | null>(null);
  let waveGeneration = $state<number | null>(null);
  let timingSettings = $state({ ...defaultTiming });
  let snapHint = $state('');
  let sceneCuts = $state<number[]>([]);
  let waveError = $state('');
  let waveWidth = $state(600);
  let waveKey = '';
  let waveRequest = 0;
  const waveCues = $derived(
    doc.cues
      .map((c) => (c.id === timingEdit?.preview.id ? timingEdit.preview : c))
      .filter(
        (c) =>
          (c.endMs + timingSettings.cueGapMs) / 1000 >= start &&
          (c.startMs - timingSettings.cueGapMs) / 1000 <= start + span,
      ),
  );
  let scrollTop = $state(0);
  let listHeight = $state(150);
  let list: HTMLDivElement;
  const firstRow = $derived(Math.max(0, Math.floor(scrollTop / 32) - 3));
  const visibleCues = $derived(
    doc.cues.slice(firstRow, firstRow + Math.ceil(listHeight / 32) + 6),
  );
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
  const qualityRows = $derived(
    new Map(quality.view?.metrics.map((m) => [m.data.cueId, m]) ?? []),
  );
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
  const duration = $derived(
    Math.max(status.durationSeconds ?? 0, wave?.durationSeconds ?? 0),
  );
  let confirmation: HTMLDialogElement;
  const subtitleDecision = new PendingDecision<'save' | 'discard' | 'cancel'>(
    'cancel',
  );
  let confirming = $state(false);
  let projectExitError = $state<string | null>(null);
  const projectExitDecision = new PendingDecision<ProjectExitChoice>('cancel');
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
      wave = null;
      waveGeneration = null;
      range = null;
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
      span = Math.max(
        0.01,
        Math.min(
          options.waveformSpan,
          status.durationSeconds ?? options.waveformSpan,
        ),
      );
      start = Math.max(
        0,
        Math.min(
          options.waveformStart,
          Math.max(0, (status.durationSeconds ?? 0) - span),
        ),
      );
    },
    restoreScenes: (session, generation) => {
      restoredScenes = { session, generation };
    },
    protect,
    saveSubtitles: () => save(false),
    saveFailed: projectSaveFailed,
    notify: (message) => {
      notice = message;
    },
  });
  function occludeVideo(occluded: boolean) {
    return media.occlude(occluded);
  }
  async function confirmChanges(): Promise<'save' | 'discard' | 'cancel'> {
    confirming = true;
    try {
      // The native child must be hidden before the dialog becomes visible.
      await occludeVideo(true);
      if (!alive) {
        confirming = false;
        return 'cancel';
      }
      confirmation.showModal();
      return subtitleDecision.wait();
    } catch (e) {
      confirming = false;
      throw e;
    }
  }
  async function answer(value: 'save' | 'discard' | 'cancel') {
    if (!confirming) return;
    confirmation.close();
    confirming = false;
    await tick();
    try {
      // Diagnostics can remain open behind this dialog and still own occlusion.
      await occludeVideo(diagnostics);
      subtitleDecision.answer(alive ? value : 'cancel');
    } catch (e) {
      error = String(e);
      subtitleDecision.answer('cancel');
    }
  }
  async function protect() {
    if (!alive) return false;
    if (!doc.dirty && !draftDirty) return true;
    const choice = await confirmChanges();
    if (!alive || choice === 'cancel') return false;
    if (choice === 'save') return save(false);
    return true;
  }
  async function projectSaveFailed(error: unknown): Promise<ProjectExitChoice> {
    confirming = true;
    try {
      await occludeVideo(true);
      if (!alive) {
        confirming = false;
        return 'cancel';
      }
      const decision = projectExitDecision.wait();
      projectExitError = String(error);
      return decision;
    } catch (error) {
      confirming = false;
      throw error;
    }
  }
  async function answerProjectExit(choice: ProjectExitChoice) {
    if (!projectExitDecision.pending || projectExitError === null) return;
    projectExitError = null;
    confirming = false;
    await tick();
    try {
      await occludeVideo(diagnostics);
      projectExitDecision.answer(alive ? choice : 'cancel');
    } catch (failure) {
      error = String(failure);
      projectExitDecision.answer('cancel');
    }
  }
  function act(action: MediaAction) {
    cancelHeldTiming();
    return media.command(action);
  }
  async function selectCue(id: number) {
    // Rows use aria-disabled so transport commands do not drop keyboard focus.
    // Enforce the pending-operation guard here for both pointer and keyboard use.
    if (busy || cueMenu || !doc.cues.some((c) => c.id === id)) return;
    suspendCueFollowing();
    cancelHeldTiming();
    busy = true;
    error = '';
    try {
      await applyDraft();
      subtitles.select(id);
      const selectedCue = subtitles.cue;
      if (selectedCue)
        selectRange([selectedCue.startMs / 1000, selectedCue.endMs / 1000]);
      await revealSelection();
      if (cue && status.loaded)
        await act({ kind: 'seek', seconds: cue.startMs / 1000 });
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
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
    busy = true;
    error = '';
    try {
      if (choice === 'apply') await applyDraft();
      else subtitles.revert();
      pendingProblem = null;
    } catch (failure) {
      error = String(failure);
      return;
    } finally {
      busy = false;
    }
    if (alive) await selectCue(Number(id));
  }
  async function run(action: WorkspaceAction, event?: Event) {
    if (busy || confirming || !ready || cueMenu || waveformGesture) return;
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
    busy = true;
    error = '';
    notice = '';
    try {
      switch (action) {
        case 'resync':
          if (!status.loaded || !doc.cues.length) break;
          resyncReturn =
            document.activeElement instanceof HTMLElement
              ? document.activeElement
              : null;
          await occludeVideo(true);
          if (!alive) break;
          confirming = true;
          resyncOpen = true;
          break;
        case 'settings':
        case 'timingSettings':
          settingsReturn =
            document.activeElement instanceof HTMLElement
              ? document.activeElement
              : null;
          await occludeVideo(true);
          confirming = true;
          settingsOpen = true;
          break;
        case 'openProject':
          await projects.open();
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
            wave = null;
            waveGeneration = null;
            range = null;
            start = 0;
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
          wave = null;
          waveGeneration = null;
          range = null;
          break;
        case 'openSubtitles': {
          const path = await chooseFile('subtitles', doc.path);
          if (path && (await protect())) {
            await docAct({ kind: 'open', path, discard: true });
            scrollTop = 0;
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
      busy = false;
    }
  }
  async function revealSelection() {
    await tick();
    if (!alive || !list) return;
    const index = doc.cues.findIndex((c) => c.id === selected);
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
    busy = true;
    try {
      await act({ kind: 'track', id });
      waveKey = '';
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  function zoom(factor: number) {
    if (busy) return;
    const center = clamp(
      status.positionSeconds ?? start + span / 2,
      start,
      start + span,
    );
    const next = viewport(
      center - (span * factor) / 2,
      span * factor,
      duration || 20,
    );
    start = next.start;
    span = next.span;
  }
  $effect(() => {
    const next = viewport(start, span, duration || 20);
    if (next.start !== start) start = next.start;
    if (next.span !== span) span = next.span;
  });
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
  $effect(() => {
    const track = status.audioTracks.find((t) => t.id === status.audioTrack);
    const key =
      status.loaded && mediaPath && track
        ? `${status.session}:${track.id}:${mediaPath}`
        : '';
    if (key === waveKey) return;
    waveKey = key;
    const request = ++waveRequest;
    wave = null;
    waveError = '';
    waveGeneration = null;
    if (!key || !track) return;
    void waveformStart(mediaPath, track.audioOrdinal, status.session)
      .then((generation) => {
        if (alive && request === waveRequest) waveGeneration = generation;
      })
      .catch((e) => {
        if (alive && request === waveRequest) waveError = String(e);
      });
  });
  $effect(() => {
    const position = status.positionSeconds;
    if (
      follow &&
      !timingEdit &&
      !waveformGesture &&
      !mediaPending &&
      !status.seeking &&
      status.loaded &&
      position !== null
    ) {
      start = playbackViewportStart(position, span, duration);
    }
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
    const key = `${doc.generation}:${next}:${listHeight}`;
    untrack(() => {
      // Automatic selection never calls selectCue: that path commits and seeks.
      if (next !== selected) {
        subtitles.select(next);
      }
      if (list && key !== lastRevealedCue) {
        list.scrollTop = revealCueRow(index, list.scrollTop, listHeight);
        scrollTop = list.scrollTop;
        lastRevealedCue = key;
      }
    });
  });
  onMount(() => {
    alive = true;
    let wavePolling = false;
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
    let lastWave = '';
    async function pollWave() {
      if (wavePolling || waveGeneration === null) return;
      const generation = waveGeneration;
      const bounds = waveformRequestWindow(start, span, duration, waveWidth);
      const key = JSON.stringify([generation, bounds]);
      if (key === lastWave && !wave?.running) return;
      wavePolling = true;
      try {
        const next = await waveformView(
          generation,
          bounds.start,
          bounds.end,
          bounds.pixels,
        );
        if (
          alive &&
          generation === waveGeneration &&
          key ===
            JSON.stringify([
              waveGeneration,
              waveformRequestWindow(start, span, duration, waveWidth),
            ])
        ) {
          wave = next;
          lastWave = key;
        }
      } catch (e) {
        if (alive && generation === waveGeneration) waveError = String(e);
      } finally {
        wavePolling = false;
      }
    }
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
      if (entry) listHeight = entry.contentRect.height;
    });
    listObserver.observe(list);
    window.addEventListener('resize', layout);
    window.addEventListener('scroll', layout, true);
    const timer = setInterval(() => {
      void pollWave();
    }, 100);
    const layoutTimer = setInterval(() => void layout(), 100);
    const recoveryTimer = setInterval(() => void refreshRecovery(), 5000);
    void refreshRecovery();
    media.startPolling();
    return () => {
      alive = false;
      projects.dispose();
      subtitles.dispose();
      quality.dispose();
      media.dispose();
      subtitleDecision.dispose();
      projectExitDecision.dispose();
      waveRequest++;
      unlisten();
      observer.disconnect();
      listObserver.disconnect();
      clearInterval(timer);
      clearInterval(layoutTimer);
      clearInterval(recoveryTimer);
      ++recoveryRequest;
      window.removeEventListener('resize', layout);
      window.removeEventListener('scroll', layout, true);
    };
  });
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
  onkeydown={(e) => {
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
        suspendCueFollowing();
        timingPressedAt = performance.now();
        heldTiming = beginHeldTiming(doc, status);
        notice = heldTiming
          ? 'Tap Space to pause; hold at least 400 ms to create a cue. Escape cancels.'
          : 'Start playback with ⌘/Ctrl+Alt+Enter, then hold Space to mark a cue.';
      } else void run('play');
    }
  }}
/>
<main
  aria-busy={busy || !ready}
  class:has-recovery={recovery.entries.length > 0 || !!recovery.error}
  style:--workspace-height={`${workspaceHeight}px`}
  style:--workspace-min-height={`${368 + editorMinimum}px`}
>
  <header class="toolbar">
    <strong>VisualSubForge</strong>
    {#if !nativeMenus}
      <button
        disabled={busy || !ready || !status.available}
        onclick={() => void run('openProject')}>Open Project</button
      >
      <button
        disabled={busy || !ready || !status.loaded}
        title={projects.path ??
          'Save media, scene results, view options and resume position'}
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
      <section class="wave panel" aria-label="Waveform">
        <div class="panelbar">
          <strong>Audio waveform</strong><span class="spacer"></span>
          <SceneControls
            bind:this={sceneControls}
            autoScan={preferences.autoScanScenes}
            detail={preferences.sceneDetail}
            onstate={(value) => (sceneState = value)}
            onfinish={(message) => (notice = message)}
            padding={Math.max(
              timingSettings.sceneBeforeMs,
              timingSettings.sceneAfterMs,
            ) /
              1000 +
              (span * timingSettings.snapDistancePx) / Math.max(1, waveWidth)}
            restored={restoredScenes}
            bind:shown={showScenes}
            path={mediaPath}
            session={status.session}
            loaded={status.loaded}
            {start}
            {span}
            position={status.positionSeconds}
            disabled={busy || confirming || mediaPending || !ready}
            oncuts={(cuts) => (sceneCuts = cuts)}
            onseek={(seconds) => void seek(seconds)}
          />
          <button
            disabled={!status.loaded || busy}
            onclick={() => zoom(0.5)}
            aria-label="Zoom in waveform">+</button
          ><button
            disabled={!status.loaded || busy}
            onclick={() => zoom(2)}
            aria-label="Zoom out waveform">−</button
          ><button
            disabled={!status.loaded || busy}
            onclick={() => {
              start = 0;
              span = duration;
            }}>Fit</button
          >
        </div>
        <div class="wavecanvas" bind:this={waveHost}>
          <Waveform
            view={wave}
            {sceneCuts}
            timing={timingSettings}
            editingCueId={timingEdit?.original.id ?? null}
            enabled={!busy && !confirming && !mediaPending && status.loaded}
            {start}
            {span}
            position={status.positionSeconds}
            {duration}
            cues={waveCues}
            {range}
            {selected}
            previewRange={heldTiming
              ? [heldTiming.start, heldTiming.last]
              : timingEdit
                ? [
                    timingEdit.preview.startMs / 1000,
                    timingEdit.preview.endMs / 1000,
                  ]
                : null}
            oncontext={(x, y, cueId, inRange) =>
              void openCueMenu(x, y, cueId, inRange)}
            onseek={(t) => void seek(t)}
            onselect={(id) => void selectCue(id)}
            ongesture={(active) => (waveformGesture = active)}
            onrange={(r) => {
              if (!busy) {
                cancelHeldTiming();
                selectRange(r);
              }
            }}
            onwidth={(w) => (waveWidth = w)}
            onnavigate={(next) => {
              cancelHeldTiming();
              follow = false;
              start = next.start;
              span = next.span;
            }}
          />
          {#if timingCue && status.loaded}
            <CueTiming
              cue={timingCue}
              {start}
              {span}
              {duration}
              enabled={!busy &&
                !draftDirty &&
                !confirming &&
                ready &&
                !mediaPending}
              onbegin={beginTiming}
              onpreview={previewTiming}
              onfinish={(commit) => void finishTiming(commit)}
            />
          {/if}
          {#if !wave?.peaks.length}<div class="wave-message">
              {waveError ||
                wave?.error ||
                (wave?.running || waveGeneration !== null
                  ? 'Analyzing audio…'
                  : status.loaded
                    ? 'No mapped audio track'
                    : 'Open media to display its audio waveform')}
            </div>{/if}
        </div>
        <div class="wavefooter">
          <!-- WebKit can reset the thumb when max changes without updating the
               bound value. Recreate only when the range extent changes. -->
          <label class="pan"
            >Pan {#key Math.max(0, duration - span)}<input
                aria-label="Waveform position"
                oninput={() => (follow = false)}
                type="range"
                min="0"
                max={Math.max(0, duration - span)}
                step="0.01"
                bind:value={start}
                disabled={busy || duration <= span}
              />{/key}</label
          ><label
            ><input type="checkbox" bind:checked={follow} /> Follow playback</label
          >
          <span>{start.toFixed(2)}–{(start + span).toFixed(2)} s</span>
        </div>
      </section>
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
      <div class="transport">
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
      <div class="cue-operationbar" aria-label="Cue operations">
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
      <div class="cue-followbar">
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
          disabled={busy || confirming || !doc.loaded}
          onclick={() => void openStatistics()}>Statistics…</button
        >
        <button
          aria-expanded={problemsOpen}
          onclick={() => {
            problemsOpen = !problemsOpen;
            if (!problemsOpen) pendingProblem = null;
          }}
          >Problems{quality.view?.profileHash
            ? ` · ${quality.view.findingsCount}`
            : ''}</button
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
              timingMode = e.currentTarget.checked;
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
      <div class="subtitle-review" class:with-problems={problemsOpen}>
        <div class="cue-table">
          <div class="table-head" class:with-metrics={showQualityMetrics}>
            <span>#</span><span>Start</span><span>End</span><span
              >Subtitle text · {doc.cues.length} cues</span
            >
            {#if showQualityMetrics}<span>CPS · RS</span>{/if}
          </div>
          <div
            class="cue-list"
            bind:this={list}
            onscroll={(e) => (scrollTop = e.currentTarget.scrollTop)}
          >
            {#if !doc.loaded}<p class="empty">
                Create a cue to start new subtitles, or open an SRT file.
              </p>{:else if doc.cues.length === 0}<p class="empty">
                No cues yet. Create a cue to start editing.
              </p>{:else}
              <div
                style:height={`${doc.cues.length * 32}px`}
                class="virtual-list"
              >
                <div style:transform={`translateY(${firstRow * 32}px)`}>
                  {#each visibleCues as item, i (item.id)}<button
                      class="cue-row"
                      class:with-metrics={showQualityMetrics}
                      class:selected={item.id === selected}
                      class:active={activeIds.includes(item.id)}
                      aria-current={activeIds.includes(item.id)
                        ? 'time'
                        : undefined}
                      onfocus={suspendCueFollowing}
                      aria-pressed={item.id === selected}
                      aria-disabled={busy}
                      oncontextmenu={(e) => {
                        e.preventDefault();
                        void openCueMenu(e.clientX, e.clientY, item.id, false);
                      }}
                      onclick={() => void selectCue(item.id)}
                      ondblclick={() => void focusText()}
                      ><span>{firstRow + i + 1}</span><span
                        >{formatTime(item.startMs)}</span
                      ><span>{formatTime(item.endMs)}</span><span
                        class="cue-text"
                        >{item.text.replaceAll('\n', ' ↵ ') || '(empty)'}</span
                      >{#if showQualityMetrics}{@const metric = qualityRows.get(
                          String(item.id),
                        )}<span
                          class="row-metrics"
                          title="Accepted text · vss_utf16_v1 · unrounded values drive checks"
                          >{metric
                            ? `${rate(metric.data.cps)} · ${rate(metric.data.rs)}`
                            : '—'}</span
                        >{/if}</button
                    >{/each}
                </div>
              </div>
            {/if}
          </div>
        </div>
        {#if problemsOpen}<ProblemsPanel
            session={quality}
            disabled={busy || confirming}
            pending={pendingProblem !== null}
            onnavigate={requestProblem}
            onresolve={(choice) => void resolveProblem(choice)}
            onclose={() => {
              problemsOpen = false;
              pendingProblem = null;
            }}
          />{/if}
      </div>
    </section>
    <Splitter
      label="Resize subtitle list and editor"
      value={100 - (editorPixels / paneHeight) * 100}
      onchange={(v) =>
        (editorPct = clamp(100 - v, 20, Math.min(40, 80 - topPct)))}
    />
    <section
      class="editor panel"
      aria-label="Selected subtitle editor"
      onfocusin={(e) => {
        editorFocused =
          e.target instanceof HTMLInputElement ||
          e.target instanceof HTMLSelectElement ||
          e.target instanceof HTMLTextAreaElement;
        if (editorFocused) suspendCueFollowing();
      }}
      onfocusout={() => (editorFocused = false)}
    >
      <div class="editorbar">
        <strong
          >{cue
            ? `Cue ${doc.cues.findIndex((c) => c.id === selected) + 1}`
            : 'Cue editor'}</strong
        ><label
          >Start <input
            aria-label="Cue start"
            bind:value={subtitles.start}
            disabled={!cue}
            readonly={busy}
            spellcheck="false"
          /></label
        ><label
          >End <input
            aria-label="Cue end"
            bind:value={subtitles.end}
            disabled={!cue}
            readonly={busy}
            spellcheck="false"
          /></label
        ><button
          disabled={!cue ||
            busy ||
            !status.loaded ||
            status.positionSeconds === null}
          onclick={() => {
            if (status.positionSeconds !== null)
              subtitles.start = formatTime(
                Math.round((status.positionSeconds * 1000) / timeStepMs) *
                  timeStepMs,
              );
          }}>Start here</button
        ><button
          disabled={!cue ||
            busy ||
            !status.loaded ||
            status.positionSeconds === null}
          onclick={() => {
            if (status.positionSeconds !== null)
              subtitles.end = formatTime(
                Math.round((status.positionSeconds * 1000) / timeStepMs) *
                  timeStepMs,
              );
          }}>End here</button
        >
        {#if doc.format}
          <label
            >Style <select
              aria-label="Cue style"
              bind:value={subtitles.style}
              disabled={!cue || busy || confirming}
            >
              {#if cue && !doc.format.styles.some((style) => style.name === subtitles.style)}<option
                  value={subtitles.style}>{subtitles.style} (undefined)</option
                >{/if}
              {#each doc.format.styles as style (style.name)}<option
                  value={style.name}>{style.name}</option
                >{/each}
            </select></label
          >
          <button
            disabled={busy || confirming || !ready}
            onclick={() => void openStyles()}>Inspect styles…</button
          >
          <small
            >{doc.format.name.toUpperCase()} · {timeStepMs} ms · line breaks save
            as \N</small
          >
        {/if}
      </div>
      {#if showQualityMetrics && cue}<div class="quality-readout">
          {#if quality.view?.selected?.data.cueId === String(cue.id)}<CueMetrics
              value={quality.view.selected}
              label="Accepted"
            />{:else}<span
              >Accepted metrics {quality.paused ? 'paused' : 'pending'}…</span
            >{/if}
          {#if draftDirty}
            {#if draftMetricInput.error || quality.view?.draftError}<span
                role="status"
                >Draft metrics unavailable: {draftMetricInput.error ||
                  quality.view?.draftError}</span
              >
            {:else if quality.view?.draft?.data.cueId === String(cue.id) && !quality.stale}<CueMetrics
                value={quality.view.draft}
                label="Draft · not applied"
              />{:else}<span
                >Draft metrics {quality.paused ? 'paused' : 'pending'}…</span
              >{/if}
          {/if}
          <small
            >Source text: vss_utf16_v1 · {quality.view?.profile.speedBasis ??
              'loading profile'}. Counting excludes complete tags; it does not
            measure rendered wrapping.</small
          >
          {#if quality.error || quality.view?.error}<span role="alert"
              >{quality.error || quality.view?.error}</span
            >{/if}
        </div>{/if}
      <textarea
        aria-label="Subtitle text"
        bind:this={textEditor}
        bind:value={subtitles.text}
        disabled={!cue}
        readonly={busy}
        placeholder="Select a subtitle to edit its text."
        spellcheck="false"
        onkeydown={(e) => {
          if (
            (e.metaKey || e.ctrlKey) &&
            !e.altKey &&
            !e.shiftKey &&
            !e.repeat &&
            !e.isComposing &&
            ['b', 'i', 'u'].includes(e.key.toLowerCase())
          ) {
            e.preventDefault();
            void formatText(
              e.key.toLowerCase() === 'b'
                ? 'bold'
                : e.key.toLowerCase() === 'i'
                  ? 'italic'
                  : 'underline',
            );
            return;
          }
          if (
            (e.metaKey || e.ctrlKey) &&
            e.key === 'Enter' &&
            !e.altKey &&
            !e.shiftKey &&
            !e.repeat &&
            !e.isComposing
          ) {
            e.preventDefault();
            void run('apply', e);
          }
        }}></textarea>
      <div class="editorfooter">
        <button
          disabled={!cue || busy || confirming}
          title="Bold selection (Ctrl/Cmd+B)"
          onmousedown={(e) => e.preventDefault()}
          onclick={() => void formatText('bold')}>Bold</button
        >
        <button
          disabled={!cue || busy || confirming}
          title="Italic selection (Ctrl/Cmd+I)"
          onmousedown={(e) => e.preventDefault()}
          onclick={() => void formatText('italic')}>Italic</button
        >
        <button
          disabled={!cue || busy || confirming}
          title="Underline selection (Ctrl/Cmd+U)"
          onmousedown={(e) => e.preventDefault()}
          onclick={() => void formatText('underline')}>Underline</button
        >
        <button
          disabled={!draftDirty || busy}
          onclick={(e) => void run('apply', e)}>Apply edit</button
        ><button
          disabled={!draftDirty || busy}
          onclick={() => subtitles.revert()}>Revert draft</button
        ><button
          disabled={!doc.canUndo || draftDirty || busy}
          onclick={(e) => void run('undo', e)}>Undo edit</button
        ><button
          disabled={!doc.canRedo || draftDirty || busy}
          onclick={(e) => void run('redo', e)}>Redo edit</button
        ><span class="spacer"></span><span
          >{draftDirty
            ? 'Draft · apply to record in history'
            : doc.dirty
              ? 'Unsaved changes'
              : 'Saved state'}</span
        >
      </div>
    </section>
  </div>
  <details class="shortcut-help">
    <summary>Keyboard shortcuts</summary>
    <p>
      ⌘/Ctrl+Alt+Enter: play/pause while typing (Alt+Space also works when the
      OS allows it) · Alt+←/→: seek 1 s (Shift: 5 s) · Alt+↑/↓: edit
      previous/next cue · Enter: edit selected cue · Escape: return to waveform
      / cancel held timing. In Timing mode, tap Space to play/pause; hold it for
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
    aria-label="Unsaved subtitle changes"
  >
    <h2>Save subtitle changes?</h2>
    <p>Your current document or cue draft has changes.</p>
    <div class="dialog-actions">
      <button onclick={() => answer('cancel')}>Cancel</button><button
        onclick={() => answer('discard')}>Discard changes</button
      ><button onclick={() => answer('save')}>Save changes</button>
    </div>
  </dialog>
  {#if projectExitError !== null}
    <ProjectSaveFailureDialog
      error={projectExitError}
      onchoose={(choice) => void answerProjectExit(choice)}
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
  {#if stylesOpen && doc.format}
    <StyleInspector
      format={doc.format}
      initial={subtitles.style}
      onclose={() => void closeStyles()}
    />
  {/if}
  {#if timingOpen}
    <TimingDialog
      {subtitles}
      mediaSession={() => status.session}
      onclose={() => void closeTimingReview()}
      onaccepted={() => void closeTimingReview(true)}
    />
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
    grid-template-rows: var(--top-height) 6px minmax(220px, 1fr) 6px var(
        --editor-height
      );
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
  .wave .panelbar {
    flex-wrap: wrap;
    height: auto;
    min-height: 32px;
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
  .wave {
    display: flex;
    flex-direction: column;
  }
  .wavecanvas {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .wave-message {
    position: absolute;
    top: 36%;
    left: 12px;
    right: 12px;
    pointer-events: none;
    color: #a7bdca;
    text-align: center;
  }
  .wavefooter {
    display: flex;
    align-items: center;
    padding: 2px 8px;
    gap: 8px;
    font-size: 11px;
  }
  .pan {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: 1;
  }
  .pan input {
    width: 100%;
    padding: 0;
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
  .cue-table {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 64px;
  }
  .transport {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 8px;
    background: #203542;
    flex-wrap: wrap;
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
  }
  .cue-followbar label {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .cue-followbar button {
    padding: 2px 6px;
  }
  .table-head,
  .cue-row {
    display: grid;
    grid-template-columns: 44px 116px 116px minmax(0, 1fr);
    text-align: left;
    gap: 8px;
    align-items: center;
    padding: 0 10px;
  }
  .table-head {
    height: 25px;
    flex-shrink: 0;
    background: #304753;
    color: #bfd0da;
    font-size: 11px;
  }
  .cue-list {
    overflow: auto;
    flex: 1;
    min-height: 0;
  }
  .virtual-list {
    position: relative;
  }
  .cue-row {
    width: 100%;
    height: 32px;
    border: 0;
    border-bottom: 1px solid #29404d;
    border-radius: 0;
    background: #172934;
    font-variant-numeric: tabular-nums;
    font-size: 12px;
  }
  .cue-row.selected {
    background: #32566a;
    box-shadow: inset 3px 0 #80dfc8;
  }
  .table-head.with-metrics,
  .cue-row.with-metrics {
    grid-template-columns: 44px 116px 116px minmax(0, 1fr) 110px;
  }
  .row-metrics {
    color: #a4dccf;
    font-size: 11px;
  }
  .with-problems .table-head.with-metrics,
  .with-problems .cue-row.with-metrics {
    grid-template-columns: 26px 82px 82px minmax(60px, 1fr) 76px;
    gap: 5px;
    font-size: 11px;
  }
  .quality-readout {
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 100px;
    flex-shrink: 0;
    overflow: auto;
    font-size: 11px;
  }
  .quality-readout small {
    color: #a9c1ce;
    font-size: 10px;
  }
  .quality-readout [role='alert'] {
    color: #ffb3ad;
  }
  .cue-row.active {
    border-inline-end: 4px solid #e8c76a;
  }
  .cue-row.active > span:first-child {
    color: #ffe39a;
    font-weight: 700;
  }
  .cue-text {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .empty {
    padding: 16px;
    color: #9cb2c1;
  }
  .editor {
    display: flex;
    flex-direction: column;
    padding: 6px 8px;
    gap: 6px;
  }
  .editorbar,
  .editorfooter {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .editorbar label {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .editorbar input {
    width: 116px;
    font-variant-numeric: tabular-nums;
  }
  .editorbar select {
    max-width: 150px;
  }
  .editorbar small {
    color: #a9c1ce;
  }
  .editor textarea {
    resize: none;
    flex: 1;
    min-height: 34px;
    width: 100%;
    font-size: 16px;
    line-height: 1.4;
    padding: 8px;
  }
  .editorfooter span {
    font-size: 11px;
    color: #a9c1ce;
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
    .transport,
    .cue-operationbar,
    .cue-followbar {
      flex-shrink: 0;
    }
    .editor {
      min-height: 210px;
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
    .table-head,
    .cue-row {
      grid-template-columns: 32px 100px 100px minmax(0, 1fr);
      gap: 4px;
      padding-inline: 6px;
    }
    .table-head.with-metrics,
    .cue-row.with-metrics {
      grid-template-columns: 26px 82px 82px minmax(60px, 1fr) 76px;
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
