<script lang="ts">
  import Kbd from './Kbd.svelte';
  import type { Tooltips } from './tooltips.svelte';
  let { owner }: { owner: Tooltips } = $props();
</script>

{#if owner.content}
  {@const description = owner.description(owner.content.id)}
  <div
    role="tooltip"
    id="workspace-tooltip"
    style:left={`${owner.content.x}px`}
    style:top={`${owner.content.y}px`}
  >
    <span>{description.label}</span><Kbd shortcut={description.shortcut} />
  </div>
{/if}

<style>
  div {
    position: fixed;
    z-index: 100;
    pointer-events: none;
    display: flex;
    gap: 12px;
    max-width: 264px;
    padding: 7px 10px;
    color: var(--text);
    background: var(--surface-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    font-size: 12px;
    box-shadow: var(--shadow-pop);
  }
</style>
