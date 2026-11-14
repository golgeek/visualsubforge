<script lang="ts">
  import type { TimingGhost } from './review-preview';
  let {
    ghosts,
    start,
    span,
  }: { ghosts: TimingGhost[]; start: number; span: number } = $props();
</script>

<div class="ghosts" aria-hidden="true">
  {#each ghosts as ghost (ghost.id)}
    {#if span > 0 && ghost.endMs / 1000 >= start && ghost.startMs / 1000 <= start + span}
      {@const left = Math.max(0, ((ghost.startMs / 1000 - start) / span) * 100)}
      {@const right = Math.min(
        100,
        ((ghost.endMs / 1000 - start) / span) * 100,
      )}
      <div
        class="ghost"
        data-preview-cue={ghost.id}
        style:left={`${left}%`}
        style:width={`${Math.max(0, right - left)}%`}
      ></div>
    {/if}
  {/each}
</div>

<style>
  .ghosts {
    position: absolute;
    inset: 20px 0 3px;
    pointer-events: none;
    overflow: hidden;
  }
  .ghost {
    position: absolute;
    top: 2px;
    bottom: 2px;
    min-width: 1px;
    border: 2px dashed var(--accent);
    background: var(--accent-soft);
    box-sizing: border-box;
  }
</style>
