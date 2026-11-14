<script lang="ts">
  import type { MediaSession } from '../media/media-session.svelte';
  let { media }: { media: MediaSession } = $props();
  const status = $derived(media.status);
</script>

{#if status.subtitleFontDiagnostics.length > 0}
  <details class="shortcut-help" aria-label="Subtitle font notices">
    <summary
      >Subtitle font notices ({status.subtitleFontDiagnostics.length})</summary
    >
    <p>
      The renderer reported font issues in this media session. Fallback may
      still render the text. Check the affected subtitles and make the required
      fonts available; original document font names are unchanged. These notices
      include previously displayed cues and clear when media is closed or
      replaced.
    </p>
    <ul>
      {#each status.subtitleFontDiagnostics as message, index (index)}<li>
          {message}
        </li>{/each}
    </ul>
  </details>
{/if}

<style>
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
</style>
