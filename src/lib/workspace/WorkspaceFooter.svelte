<script lang="ts">
  import type { ExtractionSession } from '../extraction/extraction-session.svelte';
  import type { Workspace } from './workspace.svelte';
  import type { MediaSession } from '../media/media-session.svelte';
  import type { TimingGesture } from '../timing/timing-gesture.svelte';
  import type { TimelineView } from '../timeline/timeline-view.svelte';
  import { formatTime } from '../timeline/timeline';
  let {
    workspace,
    media,
    gestures,
    timeline,
    extraction,
    showExtraction = false,
  }: {
    workspace: Workspace;
    media: MediaSession;
    gestures: TimingGesture;
    timeline: TimelineView;
    extraction?: ExtractionSession;
    showExtraction?: boolean;
  } = $props();
  const status = $derived(media.status);
</script>

<footer>
  {#if showExtraction && extraction?.notice}
    <span role="status"
      >This video contains {extraction.notice.count} subtitle tracks.</span
    >
    <button
      class="notice-action"
      onclick={() => void extraction?.open(extraction.notice?.path)}
      >Extract…</button
    >
    <button
      class="notice-action"
      aria-label="Dismiss subtitle extraction notice"
      onclick={() => extraction?.dismiss()}>Dismiss</button
    >
  {/if}
  <span
    class="message"
    role="status"
    title={workspace.error ||
      status.error ||
      status.subtitlePreviewError ||
      undefined}
    >{workspace.error ||
      status.error ||
      status.subtitlePreviewError ||
      workspace.notice ||
      (workspace.busy ? 'Working…' : 'Ready')}</span
  >{#if gestures.edit}<span
      >{gestures.snapHint || 'Timing preview'}
      {formatTime(gestures.edit.preview.startMs)}–{formatTime(
        gestures.edit.preview.endMs,
      )}</span
    >{/if}{#if timeline.range}<span
      >Range {timeline.range[0].toFixed(3)}–{timeline.range[1].toFixed(3)} s</span
    >{/if}
</footer>

<style>
  footer {
    min-width: 0;
    display: flex;
    gap: 16px;
    align-items: center;
    padding: 0 12px;
    background: var(--surface-raised);
    z-index: 2;
    color: var(--text-muted);
    font-size: 11px;
  }
  .notice-action {
    padding: 0 4px;
    font-size: inherit;
  }
  .message {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @media (max-width: 799px), (max-height: 609px) {
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
  }
</style>
