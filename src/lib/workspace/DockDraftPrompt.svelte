<script lang="ts">
  import { onMount } from 'svelte';
  let {
    disabled,
    onresolve,
  }: {
    disabled: boolean;
    onresolve: (choice: 'apply' | 'discard' | 'cancel') => void;
  } = $props();
  let first: HTMLButtonElement;
  onMount(() => first.focus({ preventScroll: true }));
</script>

<div
  class="draft-choice"
  role="group"
  aria-label="Resolve cue draft before navigation"
>
  <span>The current cue has an unapplied draft.</span>
  <button bind:this={first} {disabled} onclick={() => onresolve('apply')}
    >Apply draft and navigate</button
  >
  <button {disabled} onclick={() => onresolve('discard')}
    >Discard draft and navigate</button
  >
  <button {disabled} onclick={() => onresolve('cancel')}
    >Cancel navigation</button
  >
</div>

<style>
  .draft-choice {
    flex: none;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 10px;
    padding: 6px;
    background: var(--surface-raised);
  }
</style>
