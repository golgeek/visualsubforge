<script lang="ts">
  import { getContext, onDestroy, type Snippet } from 'svelte';
  import { tooltipContext, type Tooltips } from './tooltips.svelte';
  import type { ActionId } from '../workspace/action-registry';
  let { action, children }: { action: ActionId; children: Snippet } = $props();
  const owner = getContext<Tooltips | undefined>(tooltipContext);
  let anchor: HTMLSpanElement;
  function show() {
    owner?.show(action, anchor);
  }
  function hide() {
    void owner?.hide();
  }
  onDestroy(hide);
</script>

<span
  bind:this={anchor}
  class="tooltip-anchor"
  data-tooltip-action={action}
  onpointerenter={show}
  onpointerleave={hide}
  onfocusin={show}
  onfocusout={hide}
  onclick={hide}
  onkeydown={(event) => {
    if (event.key === 'Escape') hide();
  }}
  role="presentation"
>
  {@render children()}
</span>

<style>
  .tooltip-anchor {
    display: inline-flex;
  }
</style>
