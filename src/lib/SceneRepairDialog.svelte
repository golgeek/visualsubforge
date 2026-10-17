<script lang="ts">
  import TimingRepairDialog from './TimingRepairDialog.svelte';
  import type { DocumentSession } from './document-session.svelte';
  let {
    subtitles,
    mediaSession,
    onclose,
    onaccepted,
  }: {
    subtitles: DocumentSession;
    mediaSession: () => number;
    onclose: () => void;
    onaccepted: () => void;
  } = $props();
</script>

<TimingRepairDialog
  purpose="scene"
  {subtitles}
  {mediaSession}
  {onclose}
  {onaccepted}
/>
