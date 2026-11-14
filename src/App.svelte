<script lang="ts">
  import { WorkspacePreferences } from './lib/workspace-preferences.svelte';
  import { WorkspaceScenes } from './lib/workspace-scenes.svelte';
  import { LegacyImportDecision } from './lib/legacy-import-decision.svelte';
  import { UnsavedChanges } from './lib/unsaved-changes';
  import { TranslationReference } from './lib/translation-reference';
  import {
    readProjectView,
    beginProjectRestore,
    projectMediaOpened,
    restoreProjectOptions,
  } from './lib/project-view';
  import { CueEditing } from './lib/cue-editing.svelte';
  import { WorkspaceQuality } from './lib/workspace-quality.svelte';
  import { WorkspaceMenus } from './lib/workspace-menus.svelte';
  import { WorkspaceView } from './lib/workspace-view.svelte';
  import { CueNavigation } from './lib/cue-navigation.svelte';
  import WorkspaceDialogs from './lib/WorkspaceDialogs.svelte';
  import { WorkspaceDialogsState } from './lib/workspace-dialogs.svelte';
  import { Recovery } from './lib/recovery.svelte';
  import RecoveryBanner from './lib/RecoveryBanner.svelte';
  import { CueMenu } from './lib/cue-menu.svelte';
  import FontDiagnostics from './lib/FontDiagnostics.svelte';
  import PlaybackDiagnostics from './lib/PlaybackDiagnostics.svelte';
  import WorkspaceFooter from './lib/WorkspaceFooter.svelte';
  import WorkspaceToolbar from './lib/WorkspaceToolbar.svelte';
  import { runFileAction } from './lib/workspace-file-actions';
  import {
    runMediaAction,
    seekMedia,
    selectAudioTrack,
  } from './lib/media-actions';
  import { runCueAction, formatCueText } from './lib/cue-actions';
  import { runViewAction, setTimingMode } from './lib/view-actions';
  import { workspaceKeydown, workspaceKeyup } from './lib/workspace-keys';
  import CueTable from './lib/CueTable.svelte';
  import CueEditor from './lib/CueEditor.svelte';
  import WaveformPanel from './lib/WaveformPanel.svelte';
  import { CueTableView } from './lib/cue-table-view.svelte';
  import { Workspace } from './lib/workspace.svelte';
  import { measurePaneControl } from './lib/measure-pane-control';
  import { WorkspaceLayout } from './lib/workspace-layout.svelte';
  import ProblemsPanel from './lib/ProblemsPanel.svelte';
  import TranslationPanel from './lib/TranslationPanel.svelte';
  import { TranslationSession } from './lib/translation-session.svelte';
  import LegacyImportDialog from './lib/LegacyImportDialog.svelte';
  import { QualitySession } from './lib/quality-session.svelte';
  import { isTauri } from '@tauri-apps/api/core';
  import { DocumentSession } from './lib/document-session.svelte';
  import { MediaSession } from './lib/media-session.svelte';
  import { ProjectWorkflow } from './lib/project-workflow.svelte';
  import {
    WorkspaceModal,
    type AuthoringModal,
  } from './lib/workspace-modal.svelte';
  import { TimingGesture } from './lib/timing-gesture.svelte';
  import { TimelineView } from './lib/timeline-view.svelte';
  import {
    WorkspaceDecision,
    decisionEffects,
  } from './lib/workspace-decision.svelte';
  import { onMount, tick } from 'svelte';
  import {
    type WorkspaceAction,
    type ProjectReferenceIssue,
  } from './lib/client';
  import { formatTime } from './lib/timeline';
  import CueContextMenu from './lib/CueContextMenu.svelte';
  import Splitter from './lib/Splitter.svelte';
  import ProjectSaveFailureDialog, {
    type ProjectExitChoice,
  } from './lib/ProjectSaveFailureDialog.svelte';
  import ProjectReferenceDialog from './lib/ProjectReferenceDialog.svelte';
  import { disposeWithComponent } from './lib/component-owner';
  import { captureFocus } from './lib/capture-focus';
  import { NativeVideoLayout } from './lib/native-video-layout';
  import { WorkspaceStartup } from './lib/workspace-startup';

  // DOM ports. Owners receive only the reads, scrolling and focus operations they need.

  let textEditor = $state<HTMLTextAreaElement>();

  let waveHost = $state<HTMLDivElement>();

  let video: HTMLDivElement;

  let list = $state<HTMLDivElement>();

  let confirmation: HTMLDialogElement;

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

  const draftDirty = $derived(subtitles.dirty);

  const nativeMenus = isTauri();

  // Modal presentation and unsaved-work decisions.

  const workspaceModal = disposeWithComponent(
    new WorkspaceModal(workspace, {
      occlude: (value) => media.occlude(value),
      remainingOcclusion: () => view.diagnostics,
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
        () => view.diagnostics,
        () => confirmation.close(),
      ),
    ),
  );

  const legacyDecision = disposeWithComponent(
    new LegacyImportDecision(workspace, media, () => view.diagnostics),
  );

  const projectExitDecision = disposeWithComponent(
    new WorkspaceDecision<ProjectExitChoice, string>(
      'cancel',
      workspace,
      decisionEffects(media, () => view.diagnostics),
    ),
  );

  const projectReferenceDecision = disposeWithComponent(
    new WorkspaceDecision<
      'cancel' | 'choose' | 'reload',
      ProjectReferenceIssue
    >(
      'cancel',
      workspace,
      decisionEffects(media, () => view.diagnostics),
    ),
  );

  const unsaved = disposeWithComponent(
    new UnsavedChanges(workspace, subtitles, subtitleDecision, () =>
      confirmation.showModal(),
    ),
  );

  // Timeline and cue interactions, then their mutual connections.

  const tableView = disposeWithComponent(
    new CueTableView({
      cues: () => doc.cues,
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

  const cueEditing = new CueEditing(subtitles, media, timeline);

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
      occlude: (value) => media.occlude(value),
      remainingOcclusion: () => view.diagnostics || workspace.confirming,
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
      media,
      quality,
      cueMenu,
      view,
      preferences,
      scenes,
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
    view,
  );

  const layout = new WorkspaceLayout(preferences, subtitles);

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
  };

  async function run(action: WorkspaceAction, event?: Event) {
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
    openText: (mode: 'find' | 'replace') => dialogs.openText(mode),
    requestProblem: navigation.requestProblem,
    focusWaveform: navigation.focusWaveform,
    focusText: navigation.focusText,
    revealSelection: navigation.revealSelection,
    seek: (time: number) => seekMedia(mediaActions, time),
  };

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
    videoLayout.mount(video);
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
  style:--workspace-min-height={`${148 + layout.panes.tableMinimum + layout.panes.editorMinimum}px`}
  style:--table-min-height={`${layout.panes.tableMinimum}px`}
>
  <WorkspaceToolbar
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
    bind:clientHeight={layout.workspaceHeight}
    style:--top-height={`${layout.panes.topPixels}px`}
    style:--editor-height={`${layout.panes.editorPixels}px`}
  >
    <div class="top" style:--wave-width={`${layout.wavePct}%`}>
      <WaveformPanel
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
      <section class="video-panel" aria-label="Video preview">
        <div class="panelbar">
          <strong>Video</strong><span class="filename" title={media.path}
            >{media.path.split(/[\\/]/).pop()}</span
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
      value={(layout.panes.topPixels / layout.panes.paneHeight) * 100}
      onchange={(v) => layout.resizePreview(v)}
    />
    <section class="subtitle-panel panel" aria-label="Subtitle table">
      <div
        class="transport"
        use:measurePaneControl={(height) => (layout.transportHeight = height)}
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
            onchange={(e) =>
              void selectAudioTrack(
                mediaActions,
                Number(e.currentTarget.value),
              )}
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
        use:measurePaneControl={(height) =>
          (layout.cueOperationsHeight = height)}
      >
        {#each cueEditing.operations as op (op.operation)}
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
        use:measurePaneControl={(height) =>
          (layout.cueFollowingHeight = height)}
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
          aria-expanded={view.problemsOpen}
          onclick={() =>
            view.toggleProblems(() => {
              navigation.pendingProblem = null;
            })}
          >Problems{quality.view?.profileHash
            ? ` · ${quality.view.findingsCount}`
            : ''}</button
        >
        <button
          disabled={!workspace.canAct}
          aria-expanded={view.translationOpen}
          onclick={() => view.toggleTranslation()}
          >Translation reference{doc.translation?.dirty ? ' •' : ''}</button
        >
        <label
          ><input
            type="checkbox"
            bind:checked={preferences.showQualityMetrics}
          /> Cue metrics</label
        >
        <label
          title="Tap Space to play/pause. While playing, hold Space for at least 400 ms to create a cue and keep timing. Enter edits text; Escape returns to the waveform."
        >
          <input
            type="checkbox"
            checked={gestures.mode}
            disabled={workspace.busy || workspace.confirming}
            onchange={(e) =>
              setTimingMode(gestures, navigation, e.currentTarget.checked)}
          />
          Timing mode · hold Space
        </label>
        <label
          ><input
            type="checkbox"
            checked={navigation.following !== 'off'}
            onchange={(e) => {
              navigation.setFollowing(e.currentTarget.checked);
            }}
          /> Follow active subtitle</label
        >
        {#if navigation.following === 'suspended'}
          <span
            >{draftDirty
              ? 'Apply or revert the draft to resume.'
              : 'Following suspended.'}</span
          >
          <button
            disabled={draftDirty || workspace.busy || workspace.confirming}
            onclick={() => {
              navigation.resumeFollowing();
            }}>Resume following</button
          >
        {/if}
      </div>
      <div
        class="subtitle-review"
        class:with-problems={view.problemsOpen || view.translationOpen}
      >
        <CueTable
          {subtitles}
          {quality}
          {tableView}
          {workspace}
          metrics={preferences.showQualityMetrics}
          bind:list
          actions={{
            suspendCueFollowing: navigation.suspendFollowing,
            openCueMenu: (x, y, id, inRange) => cueMenu.open(x, y, id, inRange),
            selectCue: navigation.selectCue,
            cueSelectionKey: navigation.cueSelectionKey,
            focusText: navigation.focusText,
          }}
        />
        {#if view.problemsOpen}<ProblemsPanel
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
          />{/if}
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
      value={100 - (layout.panes.editorPixels / layout.panes.paneHeight) * 100}
      onchange={(v) => layout.resizeEditor(v)}
    />
    <CueEditor
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
    diagnostics={view.diagnostics}
    bind:hardware={media.hardware}
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
  {#if projectExitDecision.payload !== null}
    <ProjectSaveFailureDialog
      error={projectExitDecision.payload}
      onchoose={(choice) => void projectExitDecision.answer(choice)}
    />
  {/if}
  {#if projectReferenceDecision.payload !== null}
    <ProjectReferenceDialog
      issue={projectReferenceDecision.payload}
      onchoose={(choice) => void projectReferenceDecision.answer(choice)}
    />
  {/if}
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
  {#if legacyDecision.payload}
    <LegacyImportDialog
      canOpen={status.available}
      onclose={(path, open) => void legacyDecision.finish(projects, path, open)}
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
