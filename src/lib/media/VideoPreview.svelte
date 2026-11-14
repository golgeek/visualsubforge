<script lang="ts">
  import TimelineStrip from '../timeline/TimelineStrip.svelte';
  import type { TimelineView } from '../timeline/timeline-view.svelte';
  import type { Cue } from '../client';
  import type { MediaSession } from './media-session.svelte';
  let {
    media,
    timeline,
    cues = [],
    selected = null,
    cuts = [],
    disabled = true,
    previousCut = false,
    nextCut = false,
    oncut = () => {},
    onseek = () => {},
    video = $bindable(),
  }: {
    media: MediaSession;
    video: HTMLDivElement | undefined;
    timeline?: TimelineView;
    cues?: Cue[];
    selected?: number | null;
    cuts?: number[];
    disabled?: boolean;
    previousCut?: boolean;
    nextCut?: boolean;
    oncut?: (backwards: boolean) => void;
    onseek?: (seconds: number) => void;
  } = $props();
  const status = $derived(media.status);
</script>

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
          : 'Video playback unavailable'}
      </p>{/if}
  </div>
  {#if timeline}<div class="scrubber">
      <button
        aria-label="Previous shot change"
        disabled={disabled || !previousCut}
        onclick={() => oncut(true)}>◂</button
      >
      <TimelineStrip
        {cues}
        {cuts}
        {selected}
        start={timeline.start}
        span={timeline.span}
        duration={timeline.duration}
        position={status.positionSeconds}
        mediaKey={status.session}
        enabled={!disabled && status.loaded}
        onmove={onseek}
        ongesture={(active) => {
          if (timeline) timeline.interacting = active;
        }}
      />
      <button
        aria-label="Next shot change"
        disabled={disabled || !nextCut}
        onclick={() => oncut(false)}>▸</button
      >
    </div>{/if}
</section>

<style>
  .scrubber {
    display: flex;
    align-items: center;
    gap: 3px;
    height: 20px;
    flex-shrink: 0;
    z-index: 2;
    background: var(--surface-timeline);
  }
  .scrubber button {
    height: 20px;
    min-height: 0;
    padding: 0 5px;
    flex-shrink: 0;
  }
  .scrubber :global(canvas) {
    flex: 1;
    width: 0;
  }
  .filename {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
    color: var(--text-muted);
  }

  .panelbar {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 4px 8px;
    background: var(--surface-raised);
    flex-shrink: 0;
  }

  .panelbar strong {
    font-size: 12px;
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
    box-shadow: 0 0 0 max(100vmax, var(--workspace-height)) var(--surface-base);
    z-index: 0;
  }

  .video-space p {
    position: absolute;
    inset: 0;
    margin: 0;
    padding: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    line-height: 14px;
    overflow: hidden;
    text-align: center;
    color: var(--text-on-video);
  }

  .video-space.covered {
    /* Only the loaded legacy renderer needs a transparent webview hole. */
    background: var(--surface-video);
  }

  @media (max-width: 799px), (max-height: 609px) {
    .filename {
      flex-basis: 100%;
    }
  }
</style>
