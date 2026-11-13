<script lang="ts">
  import type { Workspace } from './workspace.svelte';
  import type { MediaSession } from './media-session.svelte';
  import type { TimelineView } from './timeline-view.svelte';
  let {
    workspace,
    media,
    timeline,
    diagnostics,
    hardware = $bindable(),
  }: {
    workspace: Workspace;
    media: MediaSession;
    timeline: TimelineView;
    diagnostics: boolean;
    hardware: boolean;
  } = $props();
  const status = $derived(media.status);
</script>

{#if diagnostics}<aside
    class="diagnostics panel"
    aria-label="Playback diagnostics"
  >
    <div class="panelbar">
      <strong>Playback diagnostics</strong><button
        onclick={() => void workspace.run('diagnostics')}
        >Close diagnostics</button
      >
    </div>
    <label
      >Decode on next Open <select bind:value={hardware}
        ><option value={true}>Prefer hardware (software fallback)</option
        ><option value={false}>Software</option></select
      ></label
    >
    <p>
      Decoder {status.hardwareDecoder || 'none'} · Drops {status.decoderDrops ??
        '—'} / {status.outputDrops ?? '—'} · GPU swaps {status.swapCounterAvailable
        ? status.renderedFrames
        : 'unavailable'} · Session {status.session} · Audio clock {status.queuedAudioClock
        ? 'queued timeline (experimental)'
        : 'legacy'}
    </p>
    <p>
      Waveform stream {timeline.wave?.audioStreamIndex ?? '—'} · container origin
      {timeline.wave?.containerOriginSeconds ?? '—'} s
    </p>
    <p>{timeline.error || timeline.wave?.error || ''}</p>
  </aside>{/if}

<style>
  .diagnostics {
    position: absolute;
    right: 12px;
    top: 52px;
    width: min(520px, 90%);
    border: 1px solid #87b4bb;
    box-shadow: 0 8px 30px #0008;
    z-index: 5;
    padding: 10px;
    max-height: calc(100dvh - 64px);
    overflow: auto;
  }
  .diagnostics label {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  @media (max-width: 799px), (max-height: 609px) {
    .diagnostics {
      position: fixed;
      top: 12px;
      max-height: calc(100dvh - 24px);
    }
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
  .panelbar button {
    padding: 1px 8px;
  }
</style>
