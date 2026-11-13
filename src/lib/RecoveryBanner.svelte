<script lang="ts">
  import type { Recovery } from './recovery.svelte';
  import type { Workspace } from './workspace.svelte';
  let { recovery, workspace }: { recovery: Recovery; workspace: Workspace } =
    $props();
</script>

{#if recovery.status.entries.length || recovery.status.error}
  <aside class="recovery" aria-label="Subtitle recovery">
    {#if recovery.status.error}<p role="alert">
        Recovery: {recovery.status.error}
      </p>{/if}
    {#if recovery.status.entries.length}<p>
        Recovery snapshots are available. Recover opens an unsaved copy;
        originals stay unchanged.
      </p>{/if}
    {#each recovery.status.entries as entry (entry.id)}
      <div class="recovery-row">
        <span title={entry.sourcePath ?? entry.id}
          >{entry.sourcePath?.split(/[\\/]/).pop() ??
            (entry.error ? entry.id : 'Untitled subtitles')}
          {entry.capturedMs ? new Date(entry.capturedMs).toLocaleString() : ''}
          {entry.error ?? ''}</span
        >
        <button
          disabled={workspace.busy || !workspace.ready || !!entry.error}
          onclick={() => void recovery.choose(entry.id, true)}>Recover</button
        >
        {#if recovery.discardId === entry.id}
          <span>Discard this recovery snapshot?</span>
          <button
            disabled={workspace.busy}
            onclick={() => void recovery.choose(entry.id, false)}
            >Confirm discard</button
          >
          <button
            disabled={workspace.busy}
            onclick={() => (recovery.discardId = null)}>Cancel</button
          >
        {:else}
          <button
            disabled={workspace.busy}
            onclick={() => (recovery.discardId = entry.id)}>Discard…</button
          >
        {/if}
      </div>
    {/each}
  </aside>
{/if}

<style>
  .recovery {
    position: relative;
    z-index: 2;
    max-height: 130px;
    overflow: auto;
    padding: 4px 12px;
    background: #304b54;
  }
  .recovery p {
    margin: 4px 0;
  }
  .recovery-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }
  .recovery-row > span:first-child {
    flex: 1;
    overflow-wrap: anywhere;
  }
  @media (max-width: 799px), (max-height: 609px) {
    .recovery-row {
      flex-wrap: wrap;
    }
  }
</style>
