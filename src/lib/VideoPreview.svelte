<script lang="ts">
  import type { MediaSession } from './media-session.svelte';
  let {
    media,
    video = $bindable(),
  }: { media: MediaSession; video: HTMLDivElement | undefined } = $props();
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
          : 'GPU playback is unavailable in this build'}
      </p>{/if}
  </div>
</section>

<style>
  .filename {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
    color: #b4c5d0;
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

  @media (max-width: 799px), (max-height: 609px) {
    .filename {
      flex-basis: 100%;
    }
  }
</style>
