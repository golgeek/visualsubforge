<script lang="ts">
  import CueOperationsBar from '../cues/CueOperationsBar.svelte';
  import type { CueEditing } from '../cues/cue-editing.svelte';
  import type { TimingGesture } from '../timing/timing-gesture.svelte';
  import type { CueNavigation } from '../cues/cue-navigation.svelte';
  import type { WorkspacePreferences } from '../settings/workspace-preferences.svelte';
  import type { WorkspaceScenes } from '../timeline/workspace-scenes.svelte';
  import { setTimingMode } from '../workspace/view-actions';
  import Tooltip from '../ui/Tooltip.svelte';
  import type { Workspace } from '../workspace/workspace.svelte';
  import type { MediaSession } from './media-session.svelte';
  import type { TimelineView } from '../timeline/timeline-view.svelte';
  import type { WorkspaceLayout } from '../layout/workspace-layout.svelte';
  let {
    workspace,
    media,
    timeline,
    layout,
    mediaActions,
    gestures,
    navigation,
    cueEditing,
    preferences,
    scenes,
  }: {
    gestures: TimingGesture;
    navigation: CueNavigation;
    cueEditing: CueEditing;
    preferences: WorkspacePreferences;
    scenes: WorkspaceScenes;
    workspace: Workspace;
    media: MediaSession;
    timeline: TimelineView;
    layout: WorkspaceLayout;
    mediaActions: Parameters<typeof selectAudioTrack>[0];
  } = $props();
  import { selectAudioTrack } from './media-actions';
  import { formatTime } from '../timeline/timeline';
  import { measurePaneControl } from '../layout/measure-pane-control';
  const status = $derived(media.status);
  const showPlay = $derived(!status.loaded || status.paused);
  const loopAction = $derived(status.loopActive ? 'clearLoop' : 'playRange');
</script>

<div
  class="transport"
  use:measurePaneControl={(height) => (layout.transportHeight = height)}
>
  <button
    class="transport-icon"
    aria-label={showPlay ? 'Play' : 'Pause'}
    title={showPlay ? 'Play' : 'Pause'}
    disabled={!status.loaded || workspace.busy}
    onclick={() => void workspace.run('play')}
  >
    <svg aria-hidden="true" width="16" height="16" viewBox="0 0 16 16">
      <path
        fill="currentColor"
        d={showPlay ? 'M4 2l9 6-9 6z' : 'M3 2h4v12H3z M9 2h4v12H9z'}
      />
    </svg>
  </button>
  <Tooltip action={loopAction}>
    <button
      class="transport-icon"
      aria-label="Loop range"
      aria-pressed={status.loopActive}
      aria-describedby="workspace-tooltip"
      disabled={!status.loaded ||
        workspace.busy ||
        (!status.loopActive && !timeline.range)}
      onclick={() => void workspace.run(loopAction)}
    >
      <svg aria-hidden="true" width="16" height="16" viewBox="0 0 16 16">
        <path
          d="M12 5a5 5 0 1 0 1 5M9 5h4V1"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
        />
      </svg>
    </button>
  </Tooltip>
  <output
    >{status.positionSeconds === null
      ? '—'
      : formatTime(status.positionSeconds * 1000)}</output
  >
  {#if status.audioTracks.length > 1}
    <select
      aria-label="Audio track"
      value={status.audioTrack ?? ''}
      disabled={!status.loaded || workspace.busy}
      onchange={(e) =>
        void selectAudioTrack(mediaActions, Number(e.currentTarget.value))}
    >
      {#each status.audioTracks as track (track.id)}<option value={track.id}
          >{track.title}</option
        >{/each}
    </select>
  {/if}
  <span class="range" aria-label="Selection start, end and length">
    <span
      >In <b>{timeline.range ? formatTime(timeline.range[0] * 1000) : '—'}</b
      ></span
    >
    <span
      >Out <b>{timeline.range ? formatTime(timeline.range[1] * 1000) : '—'}</b
      ></span
    >
    <span
      >Length <b
        >{timeline.range
          ? formatTime((timeline.range[1] - timeline.range[0]) * 1000)
          : '—'}</b
      ></span
    >
  </span>
  <label
    ><input
      aria-label="Tap-to-time · hold Space"
      type="checkbox"
      checked={gestures.mode}
      disabled={workspace.busy || workspace.confirming}
      onchange={(e) =>
        setTimingMode(gestures, navigation, e.currentTarget.checked)}
    />Timing<span class="sr-only"> · Tap-to-time · hold Space</span></label
  >
  <label
    ><input type="checkbox" bind:checked={scenes.showScenes} />Scene changes</label
  >
  <label
    ><input
      type="checkbox"
      checked={preferences.timingSettings.enabled}
      disabled={!workspace.canAct}
      onchange={(e) => preferences.setSnapping(e.currentTarget.checked)}
    />Snap</label
  >
  <span class="spacer"></span>
  <CueOperationsBar {workspace} {cueEditing} />
</div>

<style>
  .transport {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 8px;
    background: var(--surface-raised);
    flex-wrap: wrap;
    flex-shrink: 0;
  }

  .transport-icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 27px;
    flex: none;
    padding: 0;
  }
  .transport-icon[aria-pressed='true'] {
    color: var(--accent);
    border-color: currentColor;
  }
  .transport output {
    font-variant-numeric: tabular-nums;
    min-width: 116px;
    font: 500 18px var(--font-mono);
  }

  .transport label {
    display: flex;
    gap: 5px;
    align-items: center;
  }

  .transport select {
    max-width: 90px;
  }

  .transport label {
    font-size: 11px;
    white-space: nowrap;
  }
  .range {
    display: flex;
    gap: 8px;
    font-size: 9px;
    color: var(--text-muted);
  }
  .range > span {
    display: flex;
    flex-direction: column;
  }
  .range b {
    font: 500 10px var(--font-mono);
    color: var(--text);
    min-width: 76px;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
</style>
