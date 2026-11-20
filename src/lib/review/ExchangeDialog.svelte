<script lang="ts">
  import { formatDateTime } from '../ui/date-format';
  import { kindLabel } from './presentation';
  import { onMount } from 'svelte';
  import type { ExchangePrompt, ExchangeAnswer } from './review-session.svelte';
  let {
    prompt,
    onchoose,
  }: { prompt: ExchangePrompt; onchoose: (value: ExchangeAnswer) => void } =
    $props();
  let dialog: HTMLDialogElement;
  let uncheckedPage = $state(0);
  let readable = $state(false);
  let exportName = $state('');
  onMount(() => dialog.showModal());
</script>

<dialog
  bind:this={dialog}
  oncancel={(e) => {
    e.preventDefault();
    onchoose('cancel');
  }}
>
  {#if prompt.kind === 'different'}
    <h2>Different working subtitles</h2>
    <p>
      {prompt.author || 'An unnamed reviewer'} exported this review on {formatDateTime(
        prompt.time,
      )}.
    </p>
    <p>
      Both choices merge the discussion. Use Their Version also replaces the
      working cues in one Undo step.
    </p>
    <button onclick={() => onchoose('theirs')}>Use Their Version</button>
    <button onclick={() => onchoose('mine')}>Keep Mine</button>
  {:else}
    <h2>
      {prompt.kind === 'report'
        ? 'Export Comparison Report'
        : 'Export Review Package'}
    </h2>
    <div class="progress" class:complete={!prompt.progress.unchecked.length}>
      <strong
        >You checked {prompt.progress.checked} of {prompt.progress.total}
        {prompt.progress.total === 1 ? 'change' : 'changes'}</strong
      >
      <progress
        aria-label="Checked changes"
        value={prompt.progress.checked}
        max={Math.max(1, prompt.progress.total)}
      ></progress>
    </div>
    {#if prompt.progress.unchecked.length}
      <ul class="unchecked" aria-label="Unchecked changes">
        {#each prompt.progress.unchecked.slice(uncheckedPage * 40, (uncheckedPage + 1) * 40) as row (row.key + '/' + row.occurrence)}
          <li>
            {row.kind === 'removed' ? 'was ' : ''}#{row.sourceIndex + 1} · {kindLabel(
              row.kind,
            )}
          </li>{/each}
      </ul>
      {#if prompt.progress.unchecked.length > 40}
        <button disabled={!uncheckedPage} onclick={() => uncheckedPage--}
          >Previous unchecked</button
        >
        <button
          disabled={(uncheckedPage + 1) * 40 >=
            prompt.progress.unchecked.length}
          onclick={() => uncheckedPage++}>Next unchecked</button
        >
      {/if}
    {/if}
    {#if prompt.kind === 'report'}<label
        ><input type="checkbox" bind:checked={readable} />Readable only</label
      >{/if}
    {#if readable}<p>
        Includes displayed comparisons, all comments and the names on them.
      </p>
    {:else}<p>
        Contains both subtitle versions, all comments and the names on them.{prompt.kind ===
        'report'
          ? ' This report can be opened in VSF.'
          : ''}
      </p>{/if}
    {#if prompt.dirty && !readable}<p>
        The package includes your unsaved changes.
      </p>{/if}
    {#if !readable && !prompt.author}
      <label
        >Name on this export <input
          maxlength="100"
          bind:value={exportName}
        /></label
      >
    {/if}
    <button
      disabled={!readable && !(prompt.author || exportName.trim())}
      onclick={() =>
        onchoose({
          kind: readable ? 'readable' : 'export',
          author: prompt.author || exportName.trim(),
        })}
      >{prompt.progress.unchecked.length
        ? 'Export anyway'
        : 'Continue to Save'}</button
    >
    {#if prompt.progress.unchecked.length}<button
        onclick={() => onchoose('review')}
        >Review the {prompt.progress.unchecked.length} unchecked</button
      >{/if}
  {/if}
  <button onclick={() => onchoose('cancel')}>Cancel</button>
</dialog>

<style>
  dialog {
    width: min(440px, calc(100vw - 24px));
    max-height: 90vh;
    overflow: auto;
    padding: 18px 20px;
    background: var(--surface-raised);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-pop);
  }
  h2 {
    font-size: var(--font-size-lg);
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .complete {
    color: var(--ok);
  }
  progress {
    flex: 1;
    max-width: 120px;
    height: 6px;
    accent-color: var(--ok);
  }
  .unchecked {
    max-height: 120px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    padding: 0;
    list-style: none;
  }
  li {
    padding: 4px 8px;
    font-size: 12px;
  }
  li + li {
    border-top: 1px solid var(--line);
  }
  button {
    margin: 3px;
  }
</style>
