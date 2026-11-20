<script lang="ts">
  import { onMount } from 'svelte';
  import type { ExchangePrompt, ExchangeAnswer } from './review-session.svelte';
  let {
    prompt,
    onchoose,
  }: { prompt: ExchangePrompt; onchoose: (value: ExchangeAnswer) => void } =
    $props();
  let dialog: HTMLDialogElement;
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
      {prompt.author || 'An unnamed reviewer'} exported this review on {new Date(
        prompt.time,
      ).toLocaleString()}.
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
    {#if prompt.dirty}<p>The package includes your unsaved changes.</p>{/if}
    {#if !prompt.author}
      <label
        >Name on this export <input
          maxlength="100"
          bind:value={exportName}
        /></label
      >
    {/if}
    <button
      disabled={!(prompt.author || exportName.trim())}
      onclick={() =>
        onchoose({
          kind: readable ? 'readable' : 'export',
          author: prompt.author || exportName.trim(),
        })}>Continue to Save</button
    >
  {/if}
  <button onclick={() => onchoose('cancel')}>Cancel</button>
</dialog>
