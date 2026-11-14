<script lang="ts">
  import type { Workspace } from './workspace.svelte';
  import type { MediaSession } from './media-session.svelte';
  import type { TimelineView } from './timeline-view.svelte';
  import type { WorkspaceLayout } from './workspace-layout.svelte';
  let {
    workspace,
    media,
    timeline,
    layout,
    mediaActions,
  }: {
    workspace: Workspace;
    media: MediaSession;
    timeline: TimelineView;
    layout: WorkspaceLayout;
    mediaActions: Parameters<typeof selectAudioTrack>[0];
  } = $props();
  import { selectAudioTrack } from './media-actions';
  import { formatTime } from './timeline';
  import { measurePaneControl } from './measure-pane-control';
  const status = $derived(media.status);
</script>

<div
  class="transport"
  use:measurePaneControl={(height) => (layout.transportHeight = height)}
>
  <button
    disabled={!status.loaded || workspace.busy}
    onclick={() => void workspace.run('play')}
    >{status.paused ? 'Play' : 'Pause'}</button
  ><button
    disabled={!status.loaded || workspace.busy}
    onclick={() => void workspace.run('previousFrame')}
    aria-label="Previous frame">◀|</button
  ><button
    disabled={!status.loaded || workspace.busy}
    onclick={() => void workspace.run('nextFrame')}
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
        void selectAudioTrack(mediaActions, Number(e.currentTarget.value))}
      >{#each status.audioTracks as track (track.id)}<option value={track.id}
          >{track.title}</option
        >{/each}</select
    ></label
  ><span class="spacer"></span><button
    disabled={!timeline.range || !status.loaded || workspace.busy}
    onclick={() => void workspace.run('playRange')}>Loop range</button
  ><button
    disabled={!status.loaded || workspace.busy}
    onclick={() => void workspace.run('clearLoop')}>Clear loop</button
  >
</div>

<style>
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

  .spacer {
    flex: 1;
  }
</style>
