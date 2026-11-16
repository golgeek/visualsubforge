<script lang="ts">
  import type { ReviewChange } from '../client';
  import { formatTime } from '../timeline/timeline';
  let { change }: { change: ReviewChange } = $props();
  const signed = (ms: number) =>
    `${ms >= 0 ? '+' : ''}${(ms / 1000).toFixed(3)} s`;
</script>

<p class="diff">
  {#each change.words as word, index (index)}{#if word.kind === 'removed'}<del
        >{word.text.replaceAll('\n', '↵\n')}</del
      >{:else if word.kind === 'added'}<ins
        >{word.text.replaceAll('\n', '↵\n')}</ins
      >{:else}{word.text.replaceAll('\n', '↵\n')}{/if}{/each}
</p>
{#if change.kind === 'split' || change.kind === 'merged'}
  {@const span = (c: ReviewChange['original']) =>
    [
      Math.min(...c.map((q) => q.start_ms)),
      Math.max(...c.map((q) => q.end_ms)),
    ] as const}
  {@const a = span(change.original)}{@const b = span(change.revised)}
  <small
    >Group span · In {formatTime(a[0])} → {formatTime(b[0])}, {signed(
      b[0] - a[0],
    )} · Out {formatTime(a[1])} → {formatTime(b[1])}, {signed(
      b[1] - a[1],
    )}</small
  >
  {#each [['Original', change.original], ['Revised', change.revised]] as const as [label, side] (label)}
    {#each side as cue (cue.source_index)}<small
        >{label} {formatTime(cue.start_ms)} → {formatTime(cue.end_ms)}</small
      >{/each}
  {/each}
{:else}
  {#each change.original as cue, index (cue.source_index)}
    {@const revised = change.revised[index]}
    <small
      >In {formatTime(cue.start_ms)}{#if revised}
        → {formatTime(revised.start_ms)}, {signed(
          revised.start_ms - cue.start_ms,
        )}{/if} · Out {formatTime(cue.end_ms)}{#if revised}
        → {formatTime(revised.end_ms)}, {signed(
          revised.end_ms - cue.end_ms,
        )}{/if}</small
    >
  {/each}
  {#if !change.original.length}{#each change.revised as cue (cue.source_index)}<small
        >Added: {formatTime(cue.start_ms)} → {formatTime(cue.end_ms)}</small
      >{/each}{/if}
{/if}

<style>
  .diff {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    margin: 0.3rem 0;
  }
  small {
    display: block;
  }
  del {
    text-decoration: line-through;
  }
  ins {
    text-decoration: underline;
    font-weight: 600;
  }
</style>
