<script lang="ts">
  import { onMount } from 'svelte';
  import { layoutPresets, type LayoutPreset } from './layout-presets';
  let {
    active,
    onselect,
    onclose,
  }: {
    active: LayoutPreset;
    onselect: (preset: LayoutPreset) => Promise<void>;
    onclose: () => Promise<void>;
  } = $props();
  let dialog: HTMLDialogElement;
  let pending = $state(false);
  onMount(() => dialog.showModal());
</script>

<dialog
  bind:this={dialog}
  aria-label="Workspace layout"
  oncancel={(event) => {
    event.preventDefault();
    if (!pending) void onclose();
  }}
>
  <h2>Workspace layout</h2>
  <p>Each layout remembers its pane sizes.</p>
  <div class="presets">
    {#each Object.entries(layoutPresets) as [id, preset] (id)}
      <button
        disabled={pending}
        aria-pressed={active === id}
        onclick={async () => {
          pending = true;
          await onselect(id as LayoutPreset);
        }}>{preset.label}</button
      >
    {/each}
  </div>
  <button disabled={pending} onclick={() => void onclose()}>Cancel</button>
</dialog>

<style>
  dialog {
    width: min(420px, calc(100vw - 32px));
    padding: 20px;
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    background: var(--surface-panel);
    color: var(--text);
  }
  dialog::backdrop {
    background: var(--scrim);
  }
  h2 {
    margin: 0 0 8px;
    font-size: 18px;
  }
  p {
    color: var(--text-muted);
  }
  .presets {
    display: grid;
    gap: 6px;
    margin: 16px 0;
  }
  .presets button {
    text-align: left;
    padding: 10px;
  }
  button[aria-pressed='true'] {
    border-color: var(--accent);
    background: var(--selection);
  }
</style>
