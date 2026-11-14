<script lang="ts">
  import type { Recovery } from './recovery.svelte';
  import type { Workspace } from '../workspace/workspace.svelte';
  let {
    recovery,
    workspace,
    disabled,
    onchoose = (id, copy) => recovery.choose(id, copy),
  }: {
    recovery: Recovery;
    workspace: Workspace;
    disabled?: boolean;
    onchoose?: (id: string, copy: boolean) => Promise<void>;
  } = $props();
  const blocked = $derived(disabled ?? (workspace.busy || !workspace.ready));
</script>

{#if recovery.status.entries.length || recovery.status.error}
  <aside class="recovery" aria-label="Subtitle recovery">
    <h2>Unsaved work recovered</h2>
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
          disabled={blocked || !!entry.error}
          onclick={() => void onchoose(entry.id, true)}>Recover</button
        >
        {#if recovery.discardId === entry.id}
          <span>Discard this recovery snapshot?</span>
          <button
            disabled={blocked}
            onclick={() => void onchoose(entry.id, false)}
            >Confirm discard</button
          >
          <button disabled={blocked} onclick={() => (recovery.discardId = null)}
            >Cancel</button
          >
        {:else}
          <button
            disabled={blocked}
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
    max-height: 360px;
    overflow: auto;
    padding: 16px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-lg);
    background: var(--surface-panel);
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
