<script lang="ts">
  import { onMount } from 'svelte';
  let {
    x,
    y,
    items,
    onchoose,
    onclose,
  }: {
    x: number;
    y: number;
    items: { id: string; label: string }[];
    onchoose: (id: string) => void;
    onclose: () => void;
  } = $props();
  let menu: HTMLDivElement;
  let left = $state(0),
    top = $state(0);
  onMount(() => {
    const bounds = menu.getBoundingClientRect();
    left = Math.max(8, Math.min(x, innerWidth - bounds.width - 8));
    top = Math.max(8, Math.min(y, innerHeight - bounds.height - 8));
    menu.querySelector('button')?.focus();
  });
  function key(event: KeyboardEvent) {
    const buttons = [...menu.querySelectorAll('button')];
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      event.stopPropagation();
      buttons[
        (index + (event.key === 'ArrowDown' ? 1 : buttons.length - 1)) %
          buttons.length
      ]?.focus();
    } else if (['Escape', 'Tab'].includes(event.key)) {
      event.preventDefault();
      event.stopPropagation();
      onclose();
    }
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (!menu.contains(e.target as Node)) onclose();
  }}
  onblur={onclose}
  onresize={onclose}
/>
<div
  bind:this={menu}
  class="cue-menu"
  role="menu"
  tabindex="-1"
  aria-label="Subtitle actions"
  style:left={`${left}px`}
  style:top={`${top}px`}
  onkeydown={key}
  oncontextmenu={(e) => e.preventDefault()}
>
  {#each items as item (item.id)}
    <button role="menuitem" onclick={() => onchoose(item.id)}
      >{item.label}</button
    >
  {/each}
</div>

<style>
  .cue-menu {
    position: fixed;
    z-index: 100;
    min-width: 210px;
    max-width: calc(100vw - 16px);
    max-height: calc(100vh - 16px);
    overflow: auto;
    padding: 5px;
    border: 1px solid #638299;
    border-radius: 7px;
    background: #1a2d3b;
    box-shadow: 0 5px 24px #0008;
  }
  button {
    display: block;
    width: 100%;
    text-align: left;
    background: transparent;
    border: 0;
    padding: 9px 12px;
    color: #f0f5fa;
    cursor: pointer;
    border-radius: 3px;
  }
  button:hover,
  button:focus-visible {
    background: #36556e;
    outline: 1px solid #88b8df;
  }
</style>
