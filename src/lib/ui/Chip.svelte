<script lang="ts">
  import type { Snippet } from 'svelte';
  let {
    active = false,
    disabled = false,
    onclick,
    children,
  }: {
    active?: boolean;
    disabled?: boolean;
    onclick: () => void;
    children: Snippet;
  } = $props();
</script>

<button type="button" class:active aria-pressed={active} {disabled} {onclick}
  >{@render children()}</button
>

<style>
  button {
    padding: 1px 8px;
    border-radius: 12px;
    border: 1px solid var(--line);
    color: var(--text-muted);
    background: transparent;
    white-space: nowrap;
  }
  button.active {
    border-color: var(--accent);
    color: var(--text);
    background: var(--selection);
  }
</style>
