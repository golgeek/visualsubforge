<script lang="ts">
  import type { ReviewSession } from './review-session.svelte';
  import CommentThreads from './CommentThreads.svelte';
  import { checkKey, kindLabel } from './presentation';
  import KindChip from './KindChip.svelte';
  import ChangeDetail from './ChangeDetail.svelte';
  let {
    review,
    frameRate = null,
  }: { review: ReviewSession; frameRate?: number | null } = $props();
  const label = kindLabel;
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
  const buckets = $derived(review.summary?.overview.buckets ?? []);
  const maximum = $derived(Math.max(1, ...buckets.map((b) => b.total)));
  const colours = ['added', 'removed', 'group', 'text', 'style', 'timing'];
  const counts = $derived.by(() => {
    const rows =
      review.summary?.rows.filter((r) => r.kind !== 'unchanged') ?? [];
    return Object.fromEntries(
      filters.map(([k]) => [
        k,
        rows.filter(
          (r) =>
            !k ||
            r.kind === k ||
            (r.kind === 'text_and_timing' &&
              (k === 'text' || k === 'timing')) ||
            (k === 'split' && r.kind === 'merged'),
        ).length,
      ]),
    );
  });
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
    {#if buckets.length}<div class="spark" aria-label="Changes over time">
        {#each buckets as bucket, i (i)}
          <button
            title={`${bucket.kinds.reduce((a, b) => a + b, 0)} of ${bucket.total} cues changed · ${formatTime(i * (review.summary?.overview.width_ms ?? 0))}`}
            disabled={bucket.first == null}
            style:height={`${(100 * bucket.total) / maximum}%`}
            onclick={() => {
              const row = review.summary?.rows.find(
                (r) => r.index === bucket.first,
              );
              if (row) {
                review.setTableFilter('changed');
                void review.goToRow(row);
              }
            }}
          >
            {#each bucket.kinds as n, j (j)}{#if n}<i
                  style:height={`${(100 * n) / bucket.total}%`}
                  style:background={`var(--kind-${colours[j]})`}
                ></i>{/if}{/each}
          </button>
        {/each}
      </div>{/if}
    {#if review.page.total}
      {@const index = review.page.rows.findIndex(
        (c) => review.rowId(c) === review.selectedRow,
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
        aria-label={label}
        data-count={counts[value]}
        style:--swatch={value
          ? `var(--kind-${value === 'split' ? 'group' : value})`
          : undefined}
        aria-pressed={!review.threadFilter && (review.filter ?? '') === value}
        onclick={() => {
          review.threadFilter = null;
          review.offset = 0;
          review.filter = value || null;
        }}
        >{#if value}<i class="swatch"></i>{/if}{label}</button
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
  <div class="panel-nav">
    <button onclick={() => void review.navigate(false)}>Previous Change</button
    ><button onclick={() => void review.navigate(true)}>Next Change</button>
    {#if !review.threadFilter}<button
        disabled={!review.canMutate || !review.page?.total}
        onclick={() => void review.revert(null)}
        >Revert {review.page?.matched ?? 0}
        {review.filter
          ? (filters.find(([v]) => v === review.filter)?.[1] ??
              label(review.filter)) + ' '
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
    {@const summary = review.rowSummary(change)}
    <article class:selected={review.selectedRow === review.rowId(change)}>
      {#if block}<div class="block-card">
          {#if review.page?.discussion?.blockCounts[block.block.first]}<span
              >{review.page.discussion.blockCounts[block.block.first]}
              {review.page.discussion.blockCounts[block.block.first] === 1
                ? 'thread'
                : 'threads'}</span
            >{/if}
          {#if summary && !review.expanded.includes(block.block.first)}<label
              ><input
                type="checkbox"
                aria-label="Checked shift block"
                checked={review.index.blocks
                  .get(block.block.first)
                  ?.every((r) => review.checked.has(checkKey(r)))}
                onchange={() => review.toggleCheck(summary, true)}
              /> Checked</label
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
        </div>{/if}
      {#if !block || review.expanded.includes(block.block.first)}
        <div class="card-head">
          <button
            class="select"
            aria-pressed={review.rowId(change) === review.selectedRow}
            aria-label={`Select ${label(change.kind)} change`}
            onclick={() => void review.select(change)}
          >
            <span
              >#{(change.revised[0] ?? change.original[0])!.source_index +
                1}</span
            >
            <KindChip kind={change.kind} />
          </button>
          {#if summary && change.kind !== 'unchanged'}<label
              ><input
                type="checkbox"
                aria-label="Checked change"
                checked={review.checked.has(checkKey(summary))}
                onchange={() => review.toggleCheck(summary)}
              /> Checked</label
            >{/if}
          {#if change.kind !== 'unchanged'}<button
              class="revert"
              disabled={!review.canMutate}
              onclick={() => void review.revert(change)}>Revert</button
            >{/if}
        </div>
        <ChangeDetail {change} {review} />
        <CommentThreads {review} {change} />
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
  .swatch {
    width: 7px;
    height: 7px;
    display: inline-block;
    border-radius: 2px;
    margin-right: 4px;
    background: var(--swatch);
  }
  nav {
    padding: 3px 8px;
  }
  .block-card {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px;
    font-size: 11px;
  }
  .block-card button,
  .block-card label {
    font-size: 11px;
    margin: 0;
  }
  .block-card button:first-of-type {
    border: 0;
    background: transparent;
    font-weight: 600;
    text-align: left;
  }
  .panel-nav {
    padding: 5px 8px;
  }
  .panel-nav button {
    font-size: 11px;
    padding: 1px 7px;
  }
  section[aria-label='Detached'] pre {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .spark {
    height: 34px;
    display: flex;
    align-items: flex-end;
    gap: 1px;
    border-bottom: 1px solid var(--line-strong);
    margin: 0 10px;
  }
  .spark button {
    border: 0;
    border-radius: 1px 1px 0 0;
    padding: 0;
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column-reverse;
    background: var(--surface-pressed);
  }
  nav button {
    border-radius: 999px;
    font-size: 11px;
  }
  nav button[aria-pressed='true'] {
    background: var(--selection);
    border-color: var(--accent);
  }
  nav button[data-count]::after {
    content: attr(data-count);
    padding-left: 4px;
    color: var(--text-muted);
    font-size: 10px;
  }
  article.selected {
    background: var(--selection);
    box-shadow: inset 3px 0 var(--accent);
  }
  article:hover {
    background: var(--surface-hover);
  }
  .card-head {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
  }
  .card-head label {
    margin-left: auto;
  }
  .card-head button {
    font-size: 11px;
  }
  .select {
    background: transparent;
    border: 0;
    padding: 0;
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  .select span {
    color: var(--text-muted);
    font-family: var(--font-mono);
  }
  .revert {
    opacity: 0;
  }
  article:hover .revert,
  article:focus-within .revert,
  article.selected .revert {
    opacity: 1;
  }

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
