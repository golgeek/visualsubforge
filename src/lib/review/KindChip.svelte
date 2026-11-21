<script lang="ts">
  import type { ReviewChange } from '../client';
  import { kindLabel } from './presentation';
  let { kind }: { kind: ReviewChange['kind'] } = $props();
</script>

<span class="kinds" data-kind={kind}>
  {#each kind === 'text_and_timing' ? (['text', 'timing'] as const) : [kind] as labels (labels)}
    <b class={labels}>{kindLabel(labels)}</b>
  {/each}
</span>

<style>
  .kinds {
    display: inline-flex;
    gap: 3px;
  }
  .kinds b {
    border-radius: 999px;
    padding: 1px 7px;
    font-size: 10.5px;
    font-weight: 600;
  }
  .text {
    background: var(--kind-text-soft);
    color: var(--kind-text);
  }
  .timing {
    background: var(--kind-timing-soft);
    color: var(--kind-timing);
  }
  .added {
    background: var(--kind-added-soft);
    color: var(--kind-added);
  }
  .removed {
    background: var(--kind-removed-soft);
    color: var(--kind-removed);
  }
  .split,
  .merged {
    background: var(--kind-group-soft);
    color: var(--kind-group);
  }
  .style {
    background: var(--kind-style-soft);
    color: var(--kind-style);
  }
</style>
