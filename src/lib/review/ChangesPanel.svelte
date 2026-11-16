<script lang="ts">
  import type { ReviewSession } from './review-session.svelte';
  let { review }: { review: ReviewSession } = $props();
</script>

<section aria-label="Changes">
  {#if review.error}<p role="alert">{review.error}</p>{/if}
  {#if review.page?.statistics}{@const s = review.page.statistics}
    <p>
      {s.changes} changes · text {(s.text_percent ?? 0).toFixed(1)} % · timing {(
        s.timing_percent ?? 0
      ).toFixed(1)} % · total {(s.total_percent ?? 0).toFixed(1)} %
    </p>
    {#if !s.changes}<p>No changes yet</p>{/if}
  {:else}<p>
      {review.page?.working
        ? 'Comparing…'
        : 'Attach an original to review changes'}
    </p>{/if}
  {#each review.page?.rows ?? [] as change, index (`${change.key}/${index}`)}
    <article>
      <button
        onclick={() => (review.selected = change.key)}
        aria-pressed={review.selected === change.key}>{change.kind}</button
      >
      <p class="diff">
        {#each change.words as word, wordIndex (wordIndex)}{#if word.kind === 'removed'}<del
              >{word.text.replaceAll('\n', '↵\n')}</del
            >{:else if word.kind === 'added'}<ins
              >{word.text.replaceAll('\n', '↵\n')}</ins
            >{:else}{word.text.replaceAll('\n', '↵\n')}{/if}{/each}
      </p>
      {#each change.original as cue (cue.source_index)}<small
          >Original: {cue.start_ms} → {cue.end_ms} ms</small
        >{/each}
      {#each change.revised as cue (cue.source_index)}<small
          >Working: {cue.start_ms} → {cue.end_ms} ms</small
        >{/each}
    </article>
  {/each}
  <button
    disabled={review.offset === 0}
    onclick={() => (review.offset = Math.max(0, review.offset - 100))}
    >Previous page</button
  >
  <button
    disabled={!review.page || review.offset + 100 >= review.page.total}
    onclick={() => (review.offset += 100)}>Next page</button
  >
</section>

<style>
  article {
    border-bottom: 1px solid var(--line-strong);
    padding: 0.6rem;
  }
  .diff {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
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
