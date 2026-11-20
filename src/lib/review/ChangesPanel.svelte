<script lang="ts">
  import type { ReviewSession } from './review-session.svelte';
  import CommentThreads from './CommentThreads.svelte';
  import ChangeDetail from './ChangeDetail.svelte';
  let {
    review,
    frameRate = null,
  }: { review: ReviewSession; frameRate?: number | null } = $props();
  const label = (kind: string) =>
    kind === 'text_and_timing'
      ? 'Text and timing'
      : kind.charAt(0).toUpperCase() + kind.slice(1);
  import { formatTime } from '../timeline/timeline';
  const filters = [
    ['', 'All'],
    ['text', 'Text'],
    ['timing', 'Timing'],
    ['added', 'Added'],
    ['removed', 'Removed'],
    ['split', 'Split or merged'],
    ['style', 'Style'],
  ] as const;
</script>

<section aria-label="Changes">
  {#if review.error}<p role="alert">{review.error}</p>{/if}
  {#if review.page?.statistics}{@const s = review.page.statistics}
    <p>
      {s.changes}
      {s.changes === 1 ? 'change' : 'changes'} · text {(
        s.text_percent ?? 0
      ).toFixed(1)} % · timing {(s.timing_percent ?? 0).toFixed(1)} % · total {(
        s.total_percent ?? 0
      ).toFixed(1)} % · {s.added_cues}
      added · {review.page.discussion?.open ?? 0} open {review.page.discussion
        ?.open === 1
        ? 'thread'
        : 'threads'}
    </p>
    {#if review.page.total}
      {@const index = review.page.rows.findIndex(
        (c) => c.key === review.selected,
      )}
      <p>
        {#if index >= 0}Change {review.offset + index + 1} of {review.page
            .total}{:else}Showing {review.offset + 1}–{Math.min(
            review.offset +
              review.page.rows.length +
              (review.page.discussion?.detached.length ?? 0),
            review.page.total,
          )} of {review.page.total}{/if}
      </p>
    {/if}
    {#if !s.changes}<p>No changes yet</p>{/if}
  {:else}<p>
      {review.attached || review.page?.working
        ? 'Comparing…'
        : 'Attach an original to review changes'}
    </p>{/if}
  <nav aria-label="Change filters">
    {#each filters as [value, label] (value)}<button
        aria-pressed={!review.threadFilter && (review.filter ?? '') === value}
        onclick={() => {
          review.threadFilter = null;
          review.offset = 0;
          review.filter = value || null;
        }}>{label}</button
      >{/each}
    {#each ['open', 'commented'] as const as f (f)}<button
        aria-pressed={review.threadFilter === f}
        onclick={() => {
          review.threadFilter = f;
          review.offset = 0;
          review.filter = null;
        }}>{f === 'open' ? 'Open threads' : 'Commented'}</button
      >{/each}
  </nav>
  <div>
    <button onclick={() => void review.navigate(false)}>Previous Change</button
    ><button onclick={() => void review.navigate(true)}>Next Change</button>
    {#if !review.threadFilter}<button
        disabled={!review.canMutate || !review.page?.total}
        onclick={() => void review.revert(null)}
        >Revert {review.page?.matched ?? 0}
        {review.filter
          ? label(
              filters.find(([v]) => v === review.filter)?.[1] ?? review.filter,
            ) + ' '
          : ''}{review.page?.matched === 1 ? 'Change' : 'Changes'}</button
      >{/if}
  </div>
  {#if review.options}{@const options = review.options}
    <details>
      <summary>Comparison options</summary>
      <label
        >Timing tolerance (ms) <input
          type="number"
          min="0"
          step="1"
          value={options.tolerance_ms}
          onchange={(e) => {
            const n = Number(e.currentTarget.value);
            if (Number.isInteger(n) && n >= 0 && n <= 4294967295)
              void review.setOptions({ ...options, tolerance_ms: n });
          }}
        /></label
      >
      <button
        disabled={!frameRate}
        title={!frameRate ? 'Open media with a frame rate first' : undefined}
        onclick={() => {
          if (frameRate)
            void review.setOptions({
              ...options,
              tolerance_ms: Math.floor(1000 / frameRate),
            });
        }}>Ignore timing changes under one frame</button
      >
      {#each [['ignore_tags', 'Ignore tags'], ['ignore_line_breaks', 'Ignore line-break changes'], ['ignore_whitespace', 'Ignore whitespace-only changes']] as [key, label] (key)}<label
          ><input
            type="checkbox"
            checked={options[
              key as 'ignore_tags' | 'ignore_line_breaks' | 'ignore_whitespace'
            ]}
            onchange={(e) =>
              void review.setOptions({
                ...options,
                [key!]: e.currentTarget.checked,
              })}
          />{label}</label
        >{/each}
    </details>{/if}
  {#each review.page?.rows ?? [] as change, index (`${change.key}/${index}`)}{@const block =
      review.page?.blocks?.find((b) => b.row === index)}
    <article>
      {#if block}
        {#if review.page?.discussion?.blockCounts[block.block.first]}<span
            >{review.page.discussion.blockCounts[block.block.first]}
            {review.page.discussion.blockCounts[block.block.first] === 1
              ? 'thread'
              : 'threads'}</span
          >{/if}
        <button onclick={() => void review.select(change, block)}
          >Shifted {block.block.end - block.block.first} cues by {block.block
            .delta_ms >= 0
            ? '+'
            : ''}{(block.block.delta_ms / 1000).toFixed(3)} s ({formatTime(
            block.block.revised_start_ms,
          )} – {formatTime(block.block.revised_end_ms)})</button
        >
        <button
          aria-expanded={review.expanded.includes(block.block.first)}
          onclick={() => review.toggleBlock(block.block.first)}
          >{review.expanded.includes(block.block.first)
            ? 'Collapse'
            : 'Expand'}</button
        >
        <button
          disabled={!review.canMutate}
          onclick={() => void review.revertBlock(block.block.first)}
          >Revert Block</button
        >
      {/if}
      {#if !block || review.expanded.includes(block.block.first)}
        <button
          onclick={() => void review.select(change)}
          aria-pressed={review.selected === change.key}
          >{label(change.kind)}</button
        >
        <ChangeDetail {change} />
        <CommentThreads {review} {change} />
        {#if change.kind !== 'unchanged'}
          <button
            disabled={!review.canMutate}
            onclick={() => void review.revert(change)}>Revert</button
          >{/if}
      {/if}
    </article>{/each}
  {#if review.page?.discussion?.detached.length}<section aria-label="Detached">
      <h3>Detached</h3>
      {#each review.page.discussion.detached as t (t.id)}
        <article>
          {#if t.anchor.kind === 'change'}{#each t.anchor.original as q, i (i)}<pre>Original: {q.text}</pre>{/each}{/if}
          {#each t.anchor.revised as q, i (i)}<pre>Revised: {q.text}</pre>{/each}
          <button onclick={() => review.frameThread(t)}>Go to Time</button
          ><CommentThreads {review} detached={t} />
        </article>
      {/each}
    </section>{/if}
  <button
    disabled={review.offset === 0}
    onclick={() => (review.offset = Math.max(0, review.offset - 100))}
    >Previous page</button
  ><button
    disabled={!review.page || review.offset + 100 >= review.page.total}
    onclick={() => (review.offset += 100)}>Next page</button
  >
  <label
    ><input type="checkbox" bind:checked={review.includeUnchanged} />Include
    unchanged cues in report</label
  >
  <button
    disabled={!review.page?.statistics}
    onclick={() => void review.exportReport()}>Export Comparison Report…</button
  >
</section>

<style>
  section > p {
    padding-inline: 0.6rem;
  }
  article {
    border-bottom: 1px solid var(--line-strong);
    padding: 0.6rem;
  }
  nav,
  section > div {
    display: flex;
    gap: 0.3rem;
    flex-wrap: wrap;
  }
  label {
    display: block;
    margin: 0.3rem 0;
  }
  input[type='number'] {
    width: 6rem;
  }
</style>
