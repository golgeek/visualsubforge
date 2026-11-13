<script lang="ts">
  import type { DocumentSession } from './document-session.svelte';
  import type { MediaSession } from './media-session.svelte';
  import type { Workspace } from './workspace.svelte';
  import type { ComponentProps } from 'svelte';
  import Waveform from './Waveform.svelte';
  import SceneControls from './SceneControls.svelte';
  import CueTiming from './CueTiming.svelte';
  import type { TimelineView } from './timeline-view.svelte';
  import type { TimingGesture } from './timing-gesture.svelte';
  import type { TimingSettings } from './magnetic-timing';
  import type { Preferences } from './client';
  let {
    timeline,
    gestures,
    subtitles,
    media,
    workspace,
    preferences,
    timingSettings,
    restoredScenes,
    mediaPath,
    showScenes = $bindable(),
    follow = $bindable(),
    sceneCuts = $bindable(),
    sceneControls = $bindable(),
    waveHost = $bindable(),
    onstate,
    ongesture,
    actions,
  }: {
    timeline: TimelineView;
    gestures: TimingGesture;
    subtitles: DocumentSession;
    media: MediaSession;
    workspace: Workspace;
    preferences: Preferences;
    timingSettings: TimingSettings;
    restoredScenes: { session: number; generation: number } | null;
    mediaPath: string;
    showScenes: boolean;
    follow: boolean;
    sceneCuts: number[];
    sceneControls:
      | {
          rescan: () => Promise<void>;
          cancel: () => Promise<void>;
          navigate: (backwards: boolean) => void;
        }
      | undefined;
    waveHost: HTMLDivElement | undefined;
    onstate: NonNullable<ComponentProps<typeof SceneControls>['onstate']>;
    ongesture: (active: boolean) => void;
    actions: {
      seek: (seconds: number) => Promise<void>;
      selectCue: (id: number) => Promise<void>;
      openCueMenu: (
        x: number,
        y: number,
        id: number | null,
        inRange: boolean,
      ) => Promise<void>;
    };
  } = $props();
  const status = $derived(media.status);
  const mediaPending = $derived(media.pending);
  const busy = $derived(workspace.busy);
  const confirming = $derived(workspace.confirming);
  const ready = $derived(workspace.ready);
  const selected = $derived(subtitles.selected);
  const draftDirty = $derived(subtitles.dirty);
  const timingEdit = $derived(gestures.edit);
  const heldTiming = $derived(gestures.held);
  const timingCue = $derived(gestures.edit?.preview ?? subtitles.cue);
  const start = $derived(timeline.start);
  const span = $derived(timeline.span);
  const duration = $derived(timeline.duration);
  const range = $derived(timeline.range);
  const wave = $derived(timeline.wave);
  const waveError = $derived(timeline.error);
  const waveGeneration = $derived(timeline.generation);
  const waveWidth = $derived(timeline.width);
  const waveCues = $derived(timeline.waveCues);
  function zoom(factor: number) {
    if (!busy) timeline.zoom(factor);
  }
</script>

<section class="wave panel" aria-label="Waveform">
  <div class="panelbar">
    <strong>Audio waveform</strong><span class="spacer"></span>
    <SceneControls
      bind:this={sceneControls}
      autoScan={preferences.autoScanScenes}
      detail={preferences.sceneDetail}
      {onstate}
      onfinish={(message) => (workspace.notice = message)}
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
      onseek={(seconds) => void actions.seek(seconds)}
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
        timeline.start = 0;
        timeline.span = duration;
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
          ? [timingEdit.preview.startMs / 1000, timingEdit.preview.endMs / 1000]
          : null}
      oncontext={(x, y, cueId, inRange) =>
        void actions.openCueMenu(x, y, cueId, inRange)}
      onseek={(t) => void actions.seek(t)}
      onselect={(id) => void actions.selectCue(id)}
      {ongesture}
      onrange={(r) => {
        if (!busy) {
          gestures.cancelHeld();
          timeline.selectRange(r);
        }
      }}
      onwidth={(w) => (timeline.width = w)}
      onnavigate={(next) => {
        gestures.cancelHeld();
        follow = false;
        timeline.start = next.start;
        timeline.span = next.span;
      }}
    />
    {#if timingCue && status.loaded}
      <CueTiming
        cue={timingCue}
        {start}
        {span}
        {duration}
        enabled={!busy && !draftDirty && !confirming && ready && !mediaPending}
        onbegin={() => gestures.beginBoundary()}
        onpreview={(boundary, milliseconds, pointer) =>
          gestures.preview(boundary, milliseconds, pointer)}
        onfinish={(commit) => void gestures.finish(commit)}
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
          bind:value={timeline.start}
          disabled={busy || duration <= span}
        />{/key}</label
    ><label
      ><input type="checkbox" bind:checked={follow} /> Follow playback</label
    >
    <span>{start.toFixed(2)}–{(start + span).toFixed(2)} s</span>
  </div>
</section>

<style>
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
</style>
