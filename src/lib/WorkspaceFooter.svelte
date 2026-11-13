<script lang="ts">
  import type { Workspace } from './workspace.svelte';
  import type { MediaSession } from './media-session.svelte';
  import type { TimingGesture } from './timing-gesture.svelte';
  import type { TimelineView } from './timeline-view.svelte';
  import { formatTime } from './timeline';
  let {
    workspace,
    media,
    gestures,
    timeline,
  }: {
    workspace: Workspace;
    media: MediaSession;
    gestures: TimingGesture;
    timeline: TimelineView;
  } = $props();
  const status = $derived(media.status);
</script>

<footer>
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
    >{/if}<span>Desktop PoC</span>
</footer>

<style>
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
