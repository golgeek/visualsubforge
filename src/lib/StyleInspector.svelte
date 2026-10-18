<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { DocumentSnapshot } from './client';
  let {
    format,
    initial,
    onclose,
  }: {
    format: NonNullable<DocumentSnapshot['format']>;
    initial: string;
    onclose: () => void;
  } = $props();
  let name = $state(
    untrack(() =>
      format.styles.some((s) => s.name === initial)
        ? initial
        : (format.styles[0]?.name ?? ''),
    ),
  );
  const style = $derived(format.styles.find((s) => s.name === name));
  let dialog: HTMLDialogElement;
  onMount(() => dialog.showModal());
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="style-inspector-title"
  oncancel={(event) => {
    event.preventDefault();
    onclose();
  }}
>
  <header>
    <h2 id="style-inspector-title">
      {format.name.toUpperCase()} styles and script
    </h2>
    <button onclick={onclose}>Close styles</button>
  </header>
  <p>
    Inspect the existing definitions here. Assign a style in the cue editor,
    then Apply edit to accept it.
  </p>
  <label
    >Inspect style <select aria-label="Inspect style" bind:value={name}>
      {#each format.styles as item (item.name)}<option value={item.name}
          >{item.name}</option
        >{/each}
    </select></label
  >
  {#if style}
    <table aria-label="Style fields">
      <thead><tr><th>Field</th><th>Value</th></tr></thead><tbody>
        {#each style.fields as [field, value] (field)}<tr
            ><th scope="row">{field}</th><td>{value}</td></tr
          >{/each}
      </tbody>
    </table>
  {:else}<p>No styles are defined in this document.</p>{/if}
  <details>
    <summary>Script settings</summary>
    <table aria-label="Script settings">
      <tbody>
        {#each format.scriptInfo as [field, value], index (index)}<tr
            ><th scope="row">{field}</th><td>{value}</td></tr
          >{/each}
      </tbody>
    </table>
  </details>
  <p class="note">
    Preview uses these styles and script settings. Fonts must be available to
    the player; embedded-font availability is not guaranteed. Unknown fields
    remain saved. Inline tags may override the assigned style.
  </p>
</dialog>

<style>
  dialog {
    color: #dceaf1;
    background: #172b37;
    border: 1px solid #5e8196;
    border-radius: 8px;
    padding: 18px;
    width: min(620px, 90vw);
    max-height: 85vh;
    overflow: auto;
  }
  dialog::backdrop {
    background: #0009;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  label {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  select {
    min-width: 0;
    max-width: 65%;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    margin: 12px 0;
    font-size: 13px;
  }
  th,
  td {
    text-align: left;
    padding: 4px 8px;
    border-bottom: 1px solid #355263;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  th {
    width: 40%;
  }
  .note {
    font-size: 12px;
    color: #b4cad6;
  }
</style>
