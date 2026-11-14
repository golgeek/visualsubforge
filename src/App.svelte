<script lang="ts">
  import { CueFixes } from './lib/quality/cue-fixes.svelte';
  import CueIssuesDialog from './lib/quality/CueIssuesDialog.svelte';
  import ColumnChooser from './lib/cues/ColumnChooser.svelte';
  import LayoutPicker from './lib/layout/LayoutPicker.svelte';
  // Theme: fonts, then tokens, then element defaults that use them.
  import '@fontsource-variable/inter/wght.css';
  import '@fontsource-variable/jetbrains-mono/wght.css';
  import './lib/ui/tokens.css';
  import './lib/ui/base.css';
  import TransportBar from './lib/media/TransportBar.svelte';
  import SubtitleToolbar from './lib/cues/SubtitleToolbar.svelte';
  import ActionBrowser from './lib/workspace/ActionBrowser.svelte';
  import { WorkspaceCommands } from './lib/workspace/workspace-commands';
  import WorkspaceDecisions from './lib/dialogs/WorkspaceDecisions.svelte';
  import WorkspaceLegacyImport from './lib/project/WorkspaceLegacyImport.svelte';
  import VideoPreview from './lib/media/VideoPreview.svelte';
  import { WorkspacePreferences } from './lib/settings/workspace-preferences.svelte';
  import { WorkspaceScenes } from './lib/timeline/workspace-scenes.svelte';
  import { LegacyImportDecision } from './lib/project/legacy-import-decision.svelte';
  import { UnsavedChanges } from './lib/document/unsaved-changes';
  import { TranslationReference } from './lib/translation/translation-reference';
  import {
    readProjectView,
    beginProjectRestore,
    projectMediaOpened,
    restoreProjectOptions,
  } from './lib/project/project-view';
  import { CueEditing } from './lib/cues/cue-editing.svelte';
  import { WorkspaceQuality } from './lib/quality/workspace-quality.svelte';
  import { WorkspaceMenus } from './lib/workspace/workspace-menus.svelte';
  import { WorkspaceView } from './lib/workspace/workspace-view.svelte';
  import { CueNavigation } from './lib/cues/cue-navigation.svelte';
  import WorkspaceDialogs from './lib/dialogs/WorkspaceDialogs.svelte';
  import { WorkspaceDialogsState } from './lib/dialogs/workspace-dialogs.svelte';
  import { Recovery } from './lib/document/recovery.svelte';
  import RecoveryBanner from './lib/document/RecoveryBanner.svelte';
  import { CueMenu } from './lib/cues/cue-menu.svelte';
  import FontDiagnostics from './lib/authoring/FontDiagnostics.svelte';
  import PlaybackDiagnostics from './lib/media/PlaybackDiagnostics.svelte';
  import WorkspaceFooter from './lib/workspace/WorkspaceFooter.svelte';
  import WorkspaceToolbar from './lib/workspace/WorkspaceToolbar.svelte';
  import { runFileAction } from './lib/workspace/workspace-file-actions';
  import { runMediaAction, seekMedia } from './lib/media/media-actions';
  import { runCueAction, formatCueText } from './lib/cues/cue-actions';
  import { runViewAction } from './lib/workspace/view-actions';
  import {
    workspaceKeydown,
    workspaceKeyup,
  } from './lib/workspace/workspace-keys';
  import CueTable from './lib/cues/CueTable.svelte';
  import CueEditor from './lib/cues/CueEditor.svelte';
  import WaveformPanel from './lib/timeline/WaveformPanel.svelte';
  import { CueTableView } from './lib/cues/cue-table-view.svelte';
  import { Workspace } from './lib/workspace/workspace.svelte';
  import { WorkspaceLayout } from './lib/layout/workspace-layout.svelte';
  import ReviewDock from './lib/workspace/ReviewDock.svelte';
  import type { TimingPreview } from './lib/timing/review-preview';
  import ProblemsPanel from './lib/quality/ProblemsPanel.svelte';
  import TranslationPanel from './lib/translation/TranslationPanel.svelte';
  import { TranslationSession } from './lib/translation/translation-session.svelte';
  import { QualitySession } from './lib/quality/quality-session.svelte';
  import { isTauri } from '@tauri-apps/api/core';
  import { DocumentSession } from './lib/document/document-session.svelte';
  import { MediaSession } from './lib/media/media-session.svelte';
  import { ProjectWorkflow } from './lib/project/project-workflow.svelte';
  import {
    WorkspaceModal,
    type AuthoringModal,
  } from './lib/dialogs/workspace-modal.svelte';
  import { TimingGesture } from './lib/timing/timing-gesture.svelte';
  import { TimelineView } from './lib/timeline/timeline-view.svelte';
  import {
    WorkspaceDecision,
    decisionEffects,
  } from './lib/dialogs/workspace-decision.svelte';
  import { onMount, tick, setContext } from 'svelte';
  import { Tooltips, tooltipContext } from './lib/ui/tooltips.svelte';
  import TooltipLayer from './lib/ui/TooltipLayer.svelte';
  import {
    type WorkspaceAction,
    type ProjectReferenceIssue,
  } from './lib/client';
  import CueContextMenu from './lib/cues/CueContextMenu.svelte';
  import Splitter from './lib/layout/Splitter.svelte';
  import type { ProjectExitChoice } from './lib/project/ProjectSaveFailureDialog.svelte';
  import { disposeWithComponent } from './lib/workspace/component-owner';
  import { captureFocus } from './lib/dialogs/capture-focus';
  import { NativeVideoLayout } from './lib/media/native-video-layout';
  import { WorkspaceStartup } from './lib/workspace/workspace-startup';

  // DOM ports. Owners receive only the reads, scrolling and focus operations they need.

  let textEditor = $state<HTMLTextAreaElement>();

  let waveHost = $state<HTMLDivElement>();

  let video = $state<HTMLDivElement>();

  let list = $state<HTMLDivElement>();

  let confirmation = $state<HTMLDialogElement>();

  // Suppress workspace error continuations first. All teardown guards are invalidated synchronously;
  // decision answers only queue continuations, which then see every owner disposed.

  const workspace = disposeWithComponent(new Workspace());

  // Document, media and view state.

  const view = new WorkspaceView();

  const preferences = new WorkspacePreferences();

  const scenes = new WorkspaceScenes();

  const subtitles = disposeWithComponent(new DocumentSession());

  const media = disposeWithComponent(
    new MediaSession((failure) => {
      if (!workspace.disposed) workspace.error = String(failure);
    }),
  );

  const translation = disposeWithComponent(
    new TranslationSession(subtitles, undefined, () => view.translationOpen),
  );

  const quality = disposeWithComponent(new QualitySession());

  const status = $derived(media.status);

  const doc = $derived(subtitles.snapshot);

  const nativeMenus = isTauri();

  const tooltips: Tooltips = disposeWithComponent(
    new Tooltips(
      {
        occlude: (value) => media.occlude(value),
        remainingOcclusion: () =>
          view.diagnostics ||
          workspace.confirming ||
          cueMenu.active ||
          layout.active,
        afterRender: tick,
      },
      (error) => {
        if (!workspace.disposed) workspace.error = String(error);
      },
      () => !workspace.confirming && !workspace.busy && !cueMenu.active,
    ),
  );
  setContext(tooltipContext, tooltips);

  // Retire hover presentation before publishing another interactive overlay.
  async function occludeOverlay(value: boolean) {
    if (value) await tooltips.hide();
    await media.occlude(value);
  }

  // Modal presentation and unsaved-work decisions.

  const workspaceModal = disposeWithComponent(
    new WorkspaceModal(workspace, {
      occlude: occludeOverlay,
      remainingOcclusion: () =>
        view.diagnostics || tooltips.active || layout.active,
      afterRender: tick,
      captureFocus,
    }),
  );

  const subtitleDecision = disposeWithComponent(
    new WorkspaceDecision<'save' | 'discard' | 'cancel'>(
      'cancel',
      workspace,
      decisionEffects(
        media,
        () => view.diagnostics || tooltips.active || layout.active,
        () => confirmation!.close(),
      ),
    ),
  );

  const legacyDecision = disposeWithComponent(
    new LegacyImportDecision(
      workspace,
      media,
      () => view.diagnostics || tooltips.active || layout.active,
    ),
  );

  const projectExitDecision = disposeWithComponent(
    new WorkspaceDecision<ProjectExitChoice, string>(
      'cancel',
      workspace,
      decisionEffects(
        media,
        () => view.diagnostics || tooltips.active || layout.active,
      ),
    ),
  );

  const projectReferenceDecision = disposeWithComponent(
    new WorkspaceDecision<
      'cancel' | 'choose' | 'reload',
      ProjectReferenceIssue
    >(
      'cancel',
      workspace,
      decisionEffects(
        media,
        () => view.diagnostics || tooltips.active || layout.active,
      ),
    ),
  );

  const unsaved = disposeWithComponent(
    new UnsavedChanges(workspace, subtitles, subtitleDecision, () =>
      confirmation!.showModal(),
    ),
  );

  // Timeline and cue interactions, then their mutual connections.

  const tableView = disposeWithComponent(
    new CueTableView({
      cues: () => doc.cues,
      columns: () => layout.columns,
      styled: () => subtitles.styled,
      issues: () => quality.issues,
      position: () =>
        status.loaded && !media.pending && !status.seeking
          ? status.positionSeconds
          : null,
    }),
  );

  const timeline = disposeWithComponent(
    new TimelineView({
      status: () => status,
      mediaPath: () => media.path,
      pending: () => media.pending,
      cues: () => doc.cues,
      gap: () => preferences.timingSettings.cueGapMs,
      selectRange: (session, next) => media.loop.update(session, next),
    }),
  );

  const cueEditing = disposeWithComponent(
    new CueEditing(subtitles, media, timeline, {
      canCommit: () => workspace.canAct,
      leaveEditor: () => waveHost?.querySelector('canvas')?.focus(),
    }),
  );

  const gestures = disposeWithComponent(
    new TimingGesture(subtitles, media, timeline, workspace, {
      cuts: () => scenes.sceneCuts,
      settings: () => preferences.timingSettings,
      operationContext: () => cueEditing.context,
      command: (action, plan) =>
        subtitles.command(action, plan ? { plan } : {}),
    }),
  );

  const navigation = disposeWithComponent(
    new CueNavigation(
      workspace,
      subtitles,
      media,
      timeline,
      tableView,
      gestures,
      {
        readList: () =>
          list
            ? { scrollTop: list.scrollTop, height: list.clientHeight }
            : null,
        scrollList: (top) => {
          if (!list) return 0;
          list.scrollTop = top;
          return list.scrollTop;
        },
        focusCue: (id) =>
          list
            ?.querySelector<HTMLButtonElement>(`[data-cue-id="${id}"]`)
            ?.focus(),
        focusText: () => textEditor?.focus(),
        focusWaveform: () => waveHost?.querySelector('canvas')?.focus(),
      },
    ),
  );

  const cueMenu = disposeWithComponent(
    new CueMenu(workspace, subtitles, media, timeline, gestures, navigation, {
      occlude: occludeOverlay,
      remainingOcclusion: () =>
        view.diagnostics ||
        tooltips.active ||
        layout.active ||
        workspace.confirming,
      afterRender: tick,
      captureFocus,
      operationContext: () => cueEditing.context,
    }),
  );

  gestures.connect(navigation, cueMenu);
  navigation.connectMenu(cueMenu);
  timeline.connectPreview(() => gestures.edit?.preview ?? null);

  // Feature workflows and projections.

  const dialogs = disposeWithComponent(
    new WorkspaceDialogsState(
      workspace,
      workspaceModal,
      subtitles,
      quality,
      cueMenu,
      view,
      preferences,
      timeline,
      {
        requestProblem: navigation.requestProblem,
        selectCue: navigation.selectCue,
        suspendFollowing: navigation.suspendFollowing,
        hasTextEditor: () => !!textEditor,
        selectText: (start, end) => {
          textEditor?.focus();
          textEditor?.setSelectionRange(start, end);
        },
      },
    ),
  );

  const projectViewOwners = {
    media,
    preferences,
    scenes,
    timeline,
    navigation,
  };

  const projects = disposeWithComponent(
    new ProjectWorkflow(subtitles, media, {
      read: () => readProjectView(projectViewOwners),
      beginRestore: () => beginProjectRestore(projectViewOwners),
      mediaOpened: (path) => projectMediaOpened(projectViewOwners, path),
      restoreOptions: (options) =>
        restoreProjectOptions(projectViewOwners, options),
      restoreScenes: (session, generation) =>
        scenes.restore(session, generation),
      protect: unsaved.protect,
      saveSubtitles: () => unsaved.save(false),
      saveFailed: (failure) => projectExitDecision.request(String(failure)),
      referenceIssue: (issue) => projectReferenceDecision.request(issue),
      notify: (message) => {
        workspace.notice = message;
      },
    }),
  );

  const reference = disposeWithComponent(
    new TranslationReference(
      workspace,
      subtitles,
      translation,
      unsaved,
      preferences,
      () => projects.save(false),
    ),
  );

  const recovery = disposeWithComponent(
    new Recovery(workspace, subtitles, {
      protect: () => unsaved.protect(() => projects.save(false)),
      suspendFollowing: () => navigation.suspendFollowing(),
    }),
  );

  const workspaceQuality = new WorkspaceQuality(
    workspace,
    subtitles,
    media,
    quality,
    tableView,
    preferences,
    scenes,
  );

  let timingPreview = $state.raw<TimingPreview | null>(null);
  const reviewGhosts = $derived(
    timingPreview?.document === subtitles.snapshot &&
      !subtitles.dirty &&
      view.problemsOpen &&
      ['timing', 'shift'].includes(view.reviewTab)
      ? timingPreview.ghosts
      : [],
  );
  const reviewPreviewIds = () => [
    ...new Set(
      [...tableView.visibleCues, ...timeline.waveCues].map((cue) => cue.id),
    ),
  ];
  const cueFixes = disposeWithComponent(
    new CueFixes(() => {
      const issues = quality.view?.selectedIssues;
      if (
        !workspace.ready ||
        workspaceModal.active ||
        (view.problemsOpen && ['timing', 'shift'].includes(view.reviewTab)) ||
        workspace.confirming ||
        subtitles.dirty ||
        !subtitles.stateConfirmed ||
        quality.paused ||
        quality.stale ||
        !subtitles.cue ||
        issues?.cueId !== String(subtitles.selected) ||
        !issues.count
      )
        return null;
      return {
        document: subtitles.snapshot,
        cueId: subtitles.cue.id,
        session: media.status.session,
        context: `${preferences.qualityContext}/${scenes.sceneState.qualityKey}/${quality.view?.key.epoch}`,
      };
    }, subtitles),
  );

  const layout: WorkspaceLayout = disposeWithComponent(
    new WorkspaceLayout(preferences, subtitles, {
      occlude: occludeOverlay,
      remainingOcclusion: () =>
        view.diagnostics ||
        tooltips.active ||
        workspace.confirming ||
        cueMenu.active,
      afterRender: async () => {
        await tick();
        await videoLayout.flush();
      },
      error: (error) => {
        workspace.error = String(error);
      },
      preset: (preset) => {
        view.problemsOpen = preset === 'review';
        view.translationOpen = preset === 'translation';
        if (!view.problemsOpen) navigation.pendingProblem = null;
      },
    }),
  );
  $effect(() => {
    if (workspace.ready) layout.initialize(preferences.preferences.layout);
  });

  // Action wiring and native platform lifetimes.

  const fileActions = {
    workspace,
    subtitles,
    projects,
    gestures,
    tableView,
    legacyDecision,
    protect: () => unsaved.protect(() => projects.save(false)),
    save: unsaved.save,
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
    preferences,
    scenes,
  };

  const cueActions = {
    workspace,
    subtitles,
    gestures,
    get operationContext() {
      return cueEditing.context;
    },
    suspendFollowing: navigation.suspendFollowing,
    revealSelection: navigation.revealSelection,
    focusText: navigation.focusText,
  };

  const viewActions = {
    workspace,
    subtitles,
    media,
    gestures,
    workspaceModal,
    openAuthoring: (kind: AuthoringModal) => dialogs.openAuthoring(kind),
    view,
    scenes,
    layout,
    remainingOcclusion: () => tooltips.active,
  };

  async function run(action: WorkspaceAction, event?: Event) {
    // Preserve synchronous admission when no tooltip is presenting. A pending
    // delay is cancelled immediately; only an active native hide needs waiting.
    const tooltipActive = tooltips.active;
    const tooltipClosed = tooltips.hide();
    if (tooltipActive) await tooltipClosed;
    if (!workspace.canAct || cueMenu.context || timeline.interacting) return;
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

  const keys = {
    workspace,
    gestures,
    subtitles,
    media,
    timeline,
    quality,
    navigation,
    get menu() {
      return !!cueMenu.context;
    },
    get waveformGesture() {
      return timeline.interacting;
    },
    get problemsOpen() {
      return view.problemsOpen;
    },
    windows: __WINDOWS__,
    openCommands: (mode: 'commands' | 'shortcuts') => commands.open(mode),
    openText: (mode: 'find' | 'replace') => dialogs.openText(mode),
    requestProblem: navigation.requestProblem,
    focusWaveform: navigation.focusWaveform,
    focusText: navigation.focusText,
    revealSelection: navigation.revealSelection,
    seek: (time: number) => seekMedia(mediaActions, time),
  };

  const commands: WorkspaceCommands = new WorkspaceCommands({
    layout,
    workspace,
    subtitles,
    media,
    scenes,
    timeline,
    view,
    quality,
    cueEditing,
    navigation,
    dialogs,
    workspaceModal,
    gestures,
    cueMenu,
    keys: () => keys,
  });
  const commandMode = $derived(
    workspaceModal.identity === 'commands' ||
      workspaceModal.identity === 'shortcuts' ||
      workspaceModal.identity === 'actionMenu'
      ? workspaceModal.identity
      : null,
  );

  const menus = disposeWithComponent(
    new WorkspaceMenus(
      workspace,
      subtitles,
      media,
      view,
      scenes,
      timeline,
      navigation,
      cueEditing,
    ),
  );

  const startup = disposeWithComponent(
    new WorkspaceStartup(workspace, subtitles, preferences),
  );
  const videoLayout = disposeWithComponent(
    new NativeVideoLayout(workspace, media),
  );
  workspace.connect({
    activity: () =>
      layout.active ||
      view.reviewBusy ||
      workspaceModal.opening ||
      gestures.active ||
      cueMenu.active ||
      recovery.active,
    confirming: () =>
      // Legacy opening is task-gated; preserve its visible/closing confirmation gate.
      legacyDecision.visible ||
      legacyDecision.closing ||
      subtitleDecision.active ||
      projectExitDecision.active ||
      projectReferenceDecision.active ||
      workspaceModal.active,
    run,
  });
  onMount(() => {
    startup.start();
    menus.startListening();
    timeline.startPolling();
    videoLayout.mount(video!);
    recovery.startPolling();
    media.startPolling();
  });
</script>

<svelte:window
  onblur={() => gestures.cancelHeld()}
  onkeyup={(event) => workspaceKeyup(event, gestures)}
  onfocusin={navigation.focusEntered}
  onfocusout={() => (navigation.textFocused = false)}
  onkeydown={(event) => workspaceKeydown(event, keys)}
/>
<main
  aria-busy={workspace.busy || !workspace.ready}
  class:has-recovery={recovery.status.entries.length > 0 ||
    !!recovery.status.error}
  style:--workspace-height={`${layout.workspaceHeight}px`}
  style:--workspace-min-height={`${layout.panes.minimum + 16}px`}
  style:--table-min-height={`${layout.panes.tableMinimum}px`}
>
  <WorkspaceToolbar
    onlayout={() => void workspaceModal.open('layout')}
    oncommands={() => void commands.open('actionMenu')}
    onshortcuts={() => void commands.open('shortcuts')}
    {workspace}
    {media}
    {subtitles}
    {projects}
    {nativeMenus}
    diagnostics={view.diagnostics}
    bind:showScenes={scenes.showScenes}
    sceneState={scenes.sceneState}
  />
  <RecoveryBanner {recovery} {workspace} />

  <div
    class="workspace"
    class:editor-above={layout.preset === 'editorAbove'}
    data-preset={layout.preset}
    bind:clientHeight={layout.workspaceHeight}
    style:--top-height={`${layout.panes.topPixels}px`}
    style:--editor-height={`${layout.panes.editorPixels}px`}
    style:--transport-height={`${layout.panes.transportPixels}px`}
  >
    <div class="top" style:--wave-width={`${layout.wavePct}%`}>
      <WaveformPanel
        ghosts={reviewGhosts}
        {timeline}
        {gestures}
        {subtitles}
        {media}
        {workspace}
        preferences={preferences.preferences}
        timingSettings={preferences.timingSettings}
        restoredScenes={scenes.restoredScenes}
        mediaPath={media.path}
        bind:showScenes={scenes.showScenes}
        bind:follow={timeline.follow}
        bind:sceneCuts={scenes.sceneCuts}
        bind:sceneControls={scenes.sceneControls}
        bind:waveHost
        onstate={(value) => {
          scenes.sceneState = value;
        }}
        ongesture={(active) => (timeline.interacting = active)}
        actions={{
          seek: (time: number) => seekMedia(mediaActions, time),
          selectCue: navigation.selectCue,
          openCueMenu: (x, y, id, inRange) => cueMenu.open(x, y, id, inRange),
        }}
      />
      <Splitter
        label="Resize waveform and video"
        vertical
        value={layout.wavePct}
        onchange={(v) => layout.resizeWaveform(v)}
      />
      <VideoPreview {media} bind:video />
    </div>
    <Splitter
      label="Resize preview and subtitle list"
      value={(layout.panes.topPixels / layout.panes.paneHeight) * 100}
      onchange={(v) => layout.resizePreview(v)}
    />
    <TransportBar
      {workspace}
      {media}
      {timeline}
      {layout}
      {mediaActions}
      {gestures}
      {navigation}
      {cueEditing}
      {preferences}
      {scenes}
    />
    <section class="subtitle-panel panel" aria-label="Subtitle table">
      <div
        class="subtitle-review"
        class:with-problems={view.problemsOpen || view.translationOpen}
      >
        <CueTable
          ghosts={reviewGhosts}
          {subtitles}
          {quality}
          {tableView}
          {workspace}
          {layout}
          bind:list
          actions={{
            listScrolled: navigation.listScrolled,
            columns: () => void workspaceModal.open('columns'),
            openCueMenu: (x, y, id, inRange) => cueMenu.open(x, y, id, inRange),
            selectCue: navigation.selectCue,
            cueSelectionKey: navigation.cueSelectionKey,
            focusText: navigation.focusText,
          }}
        >
          {#snippet controls()}
            <SubtitleToolbar
              {workspace}
              {subtitles}
              {preferences}
              {navigation}
            />
          {/snippet}
        </CueTable>
        {#if view.problemsOpen}<ReviewDock
            {view}
            {subtitles}
            {tableView}
            playhead={() =>
              media.status.loaded &&
              media.status.paused &&
              !media.status.seeking &&
              media.status.positionSeconds !== null
                ? Math.round(media.status.positionSeconds * 1000)
                : null}
            mediaSession={() => media.status.session}
            context={() =>
              `${preferences.qualityContext}/${scenes.sceneState.qualityKey}`}
            previewIds={reviewPreviewIds}
            enabled={() =>
              (workspace.canAct || view.reviewBusy) &&
              !workspaceModal.active &&
              !timeline.interacting}
            onpreview={(value) => (timingPreview = value)}
            onaccepted={() => {
              workspace.notice =
                'Timings accepted. Undo restores the whole change.';
              quality.restart();
            }}
            onclose={() =>
              view.closeProblems(() => (navigation.pendingProblem = null))}
            onnavigate={(id) => {
              view.revealProblems();
              navigation.requestProblem(id);
            }}
          >
            {#snippet issues()}
              <ProblemsPanel
                session={quality}
                disabled={workspace.busy || workspace.confirming}
                pending={navigation.pendingProblem !== null}
                onnavigate={navigation.requestProblem}
                onlanguage={(operation) => void dialogs.openText(operation)}
                onresolve={(choice) => void navigation.resolveProblem(choice)}
                onclose={() =>
                  view.closeProblems(() => {
                    navigation.pendingProblem = null;
                  })}
              />
            {/snippet}
          </ReviewDock>{/if}
        {#if view.translationOpen}<TranslationPanel
            {subtitles}
            session={translation}
            disabled={!workspace.canAct}
            onload={(copy) => void reference.run(copy ? 'copy' : 'attach')}
            onoffset={(offset) => void reference.run('offset', offset)}
            ondetach={() => void reference.run('detach')}
            onclose={() => (view.translationOpen = false)}
          />{/if}
      </div>
    </section>
    <Splitter
      label="Resize subtitle list and editor"
      value={layout.preset === 'editorAbove'
        ? (layout.panes.editorPixels / layout.panes.paneHeight) * 100
        : 100 - (layout.panes.editorPixels / layout.panes.paneHeight) * 100}
      onchange={(v) =>
        layout.resizeEditor(layout.preset === 'editorAbove' ? 100 - v : v)}
    />
    <CueEditor
      editing={cueEditing}
      fixes={cueFixes}
      editorAbove={layout.preset === 'editorAbove'}
      onmove={() =>
        void layout.select(
          layout.preset === 'editorAbove' ? 'timing' : 'editorAbove',
        )}
      {subtitles}
      {media}
      {quality}
      {workspace}
      metrics={preferences.showQualityMetrics}
      draftMetricError={workspaceQuality.draft.error}
      bind:textEditor
      bind:editorFocused={navigation.editorFocused}
      onmeasure={(part, height) => layout.measureEditor(part, height)}
      actions={{
        suspendCueFollowing: navigation.suspendFollowing,
        showIssues: () => void workspaceModal.open('cueIssues'),
        reviewIssue: (rule) => {
          if (
            [
              'rs_high',
              'rs_low',
              'cps_high',
              'cps_low',
              'duration_short',
              'duration_long',
              'speed_unavailable',
            ].includes(rule)
          )
            void dialogs.openTimingReview('duration');
          else if (rule === 'gap' || rule === 'overlap')
            void dialogs.openTimingReview('gap');
          else if (rule === 'scene_start' || rule === 'scene_end')
            void dialogs.openTimingReview('scene');
          else if (rule === 'line_count' || rule === 'line_length')
            void dialogs.openText({ kind: 'reflow' });
          else {
            quality.rule = rule;
            view.problemsOpen = true;
          }
        },
        openAuthoring: (kind: AuthoringModal) => dialogs.openAuthoring(kind),
        formatText: (style) =>
          formatCueText(
            {
              workspace,
              subtitles,
              alive: () => !workspace.disposed,
              editor: () => textEditor,
            },
            style,
          ),
      }}
    />
  </div>
  <FontDiagnostics {media} />
  <TooltipLayer owner={tooltips} />
  {#if workspaceModal.identity === 'cueIssues'}<CueIssuesDialog
      {quality}
      cueId={subtitles.selected}
      onclose={async () => {
        await workspaceModal.close();
        quality.selectedOffset = 0;
      }}
    />{/if}
  {#if workspaceModal.identity === 'columns'}<ColumnChooser
      {layout}
      styled={subtitles.styled}
      onclose={() => workspaceModal.close()}
    />{/if}
  {#if workspaceModal.identity === 'layout'}
    <LayoutPicker
      active={layout.preset}
      onclose={() => workspaceModal.close()}
      onselect={async (preset) => {
        await workspaceModal.close();
        if (workspace.canAct) await layout.select(preset);
      }}
    />
  {/if}
  {#if commandMode}
    <ActionBrowser
      mode={commandMode}
      context={commands.context}
      cues={doc.cues}
      onaction={(id) => commands.run(id)}
      oncue={(id) => commands.selectCue(id)}
      onclose={() => workspaceModal.close()}
    />
  {/if}
  <WorkspaceFooter {workspace} {media} {gestures} {timeline} />
  <PlaybackDiagnostics
    {workspace}
    {media}
    {timeline}
    diagnostics={view.diagnostics}
    bind:hardware={media.hardware}
  />
  <WorkspaceDecisions
    {subtitles}
    {subtitleDecision}
    {projectExitDecision}
    {projectReferenceDecision}
    bind:confirmation
  />
  <WorkspaceDialogs
    {dialogs}
    {subtitles}
    {media}
    mediaPath={media.path}
    qualityContext={preferences.qualityContext}
    sceneState={scenes.sceneState}
    preferences={preferences.preferences}
    timingSettings={preferences.timingSettings}
  />
  <WorkspaceLegacyImport {legacyDecision} {media} {projects} />
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
  main {
    height: 100dvh;
    min-height: 610px;
    display: grid;
    grid-template-rows: 40px minmax(var(--workspace-min-height), 1fr) 20px;
    isolation: isolate;
    overflow: auto;
  }

  main.has-recovery {
    grid-template-rows: 40px auto minmax(var(--workspace-min-height), 1fr) 20px;
  }

  .workspace {
    display: grid;
    grid-template-areas: 'preview' 'previewResize' 'transport' 'cues' 'editorResize' 'editor';
    grid-template-rows:
      var(--top-height) 6px var(--transport-height) minmax(
        var(--table-min-height),
        1fr
      )
      6px var(--editor-height);
    min-height: 0;
    padding: 8px;
    gap: 0;
  }

  .workspace.editor-above {
    grid-template-areas: 'preview' 'previewResize' 'transport' 'editor' 'editorResize' 'cues';
    grid-template-rows:
      var(--top-height) 6px var(--transport-height) var(--editor-height)
      6px minmax(var(--table-min-height), 1fr);
  }
  .workspace > :global(.transport) {
    grid-area: transport;
  }
  .workspace > :global(.editor) {
    grid-area: editor;
  }
  .workspace > :global([aria-label='Resize preview and subtitle list']) {
    grid-area: previewResize;
  }
  .workspace > :global([aria-label='Resize subtitle list and editor']) {
    grid-area: editorResize;
  }
  .top {
    grid-area: preview;
    display: grid;
    grid-template-columns: var(--wave-width) 6px minmax(150px, 1fr);
    min-height: 0;
  }

  .panel {
    background: var(--surface-panel);
    position: relative;
    z-index: 2;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .subtitle-panel {
    grid-area: cues;
    display: flex;
    flex-direction: column;
  }

  .subtitle-review {
    display: flex;
    flex: 1;
    min-height: 0;
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

    .workspace {
      grid-template-areas: 'preview' 'transport' 'cues' 'editor';
      grid-template-rows: auto auto minmax(320px, auto) minmax(210px, auto);
      gap: 8px;
    }

    .workspace.editor-above {
      grid-template-areas: 'preview' 'transport' 'editor' 'cues';
      grid-template-rows: auto auto minmax(210px, auto) minmax(320px, auto);
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
</style>
