<script lang="ts">
  import type { ReviewChange } from '../client';
  import type { ReviewSession } from './review-session.svelte';
  import { checkKey, signedDelta, cueWords } from './presentation';
  import KindChip from './KindChip.svelte';
  import DiffText from './DiffText.svelte';
  import { formatTime } from '../timeline/timeline';
  let {
    change,
    review,
    editor = false,
  }: {
    change: ReviewChange;
    review?: ReviewSession | undefined;
    editor?: boolean;
  } = $props();
  const row = $derived(review?.rowSummary(change));
  const deltas = $derived.by(() => {
    if (
      !change.timing_changed ||
      !change.original.length ||
      !change.revised.length
    )
      return [];
    const span = (side: ReviewChange['original']) => [
      Math.min(...side.map((q) => q.start_ms)),
      Math.max(...side.map((q) => q.end_ms)),
    ];
    const a = span(change.original),
      b = span(change.revised);
    return ['In', 'Out'].flatMap((name, i) =>
      b[i] !== a[i] ? [name + ' ' + signedDelta(b[i]! - a[i]!)] : [],
    );
  });
</script>

<div class:compare={editor}>
  {#if editor}
    <header>
      <KindChip kind={change.kind} />
      {#if row && review}<label
          ><input
            type="checkbox"
            checked={review.checked.has(checkKey(row))}
            onchange={() => review?.toggleCheck(row!)}
          /> Checked</label
        >{/if}
      <button
        disabled={!review?.canMutate}
        onclick={() => void review?.revert(change)}>Revert</button
      >
    </header>
  {/if}
  <div class:columns={editor} class:lines={!editor}>
    {#each ['original', 'revised'] as const as side (side)}
      <div class:orig={side === 'original'}>
        {#if editor}<span class="label"
            >{side === 'original' ? 'Original' : 'Revised'}</span
          >{:else}<span class="sign" aria-label={side}
            >{side === 'original' ? '−' : '+'}</span
          >{/if}
        <div>
          {#each change[side] as cue, i (cue.source_index)}
            <div>
              {#if change.original.length > 1 || change.revised.length > 1}<small
                  >#{cue.source_index + 1}
                </small>{/if}<DiffText
                words={cueWords(change, side, i)}
                {side}
              />
            </div>
            {#if editor && change.timing_changed}<small
                >{formatTime(cue.start_ms)} → {formatTime(cue.end_ms)}</small
              >{/if}
            {#if change.style_changed}<small
                >{cue.style ?? ''} {cue.actor ?? ''}</small
              >{/if}
          {:else}<small
              >{side === 'original' ? 'Not in the original' : 'Removed'}</small
            >{/each}
        </div>
      </div>
    {/each}
  </div>
  {#if deltas.length || row?.bandChange}
    <footer>
      {#each deltas as delta (delta)}<span>{delta}</span
        >{/each}{#if row?.bandChange}<span class="band">{row.bandChange}</span
        >{/if}
    </footer>
  {/if}
</div>

<style>
  .compare {
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
  }
  header,
  footer {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding: 4px 8px;
    font-size: 11px;
  }
  header {
    background: var(--surface-raised);
    color: var(--text-muted);
  }
  header label {
    margin-left: auto;
  }
  .columns {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .columns > div {
    padding: 6px 8px;
  }
  .columns > div + div {
    border-left: 1px solid var(--line);
  }
  .label,
  small {
    display: block;
    color: var(--text-muted);
    font-size: 10.5px;
  }
  .label {
    text-transform: uppercase;
    font-weight: 600;
    letter-spacing: 0.05em;
  }
  .lines > div {
    display: grid;
    grid-template-columns: 12px minmax(0, 1fr);
    gap: 4px;
    font-size: 12.5px;
  }
  .orig {
    color: var(--text-muted);
  }
  .sign {
    color: var(--kind-added);
    font-family: var(--font-mono);
  }
  .orig .sign {
    color: var(--kind-removed);
  }
  footer {
    padding-inline: 0;
  }
  .compare footer {
    padding-inline: 8px;
    border-top: 1px solid var(--line);
  }
  footer span {
    font: 10.5px var(--font-mono);
    background: var(--kind-timing-soft);
    color: var(--kind-timing);
    border-radius: 4px;
    padding-inline: 5px;
  }
  footer .band {
    color: var(--text-muted);
    background: var(--surface-raised);
  }
</style>
