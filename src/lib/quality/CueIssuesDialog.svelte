<script lang="ts">
  import { onMount } from 'svelte';
  import type { QualitySession } from './quality-session.svelte';
  import { qualityRules, qualityText } from './quality-display';
  let {
    quality,
    cueId,
    onclose,
  }: {
    quality: QualitySession;
    cueId: number | null;
    onclose: () => Promise<void>;
  } = $props();
  let dialog: HTMLDialogElement;
  const projection = $derived(
    !quality.stale && quality.view?.selectedIssues?.cueId === String(cueId)
      ? quality.view.selectedIssues
      : null,
  );
  onMount(() => dialog.showModal());
</script>

<dialog
  bind:this={dialog}
  aria-label="All cue issues"
  oncancel={(event) => {
    event.preventDefault();
    void onclose();
  }}
>
  <h2>Cue issues</h2>
  {#if projection}
    <p>{projection.count} issues · accepted subtitle</p>
    <ol start={projection.offset + 1}>
      {#each projection.issues as issue, index (index)}
        <li>
          <strong
            >{issue.rule === 'rs_high'
              ? 'Too fast'
              : issue.rule === 'rs_low'
                ? 'Too slow'
                : qualityRules.find((rule) => rule.id === issue.rule)
                    ?.label}</strong
          >
          {#each issue.findings as finding, findingIndex (findingIndex)}
            <p>
              {qualityText(finding.explanation)}
              {qualityText(finding.expected)}
            </p>
          {/each}
        </li>
      {/each}
    </ol>
    {#if projection.count > 200}
      <button
        disabled={projection.offset === 0}
        onclick={() =>
          (quality.selectedOffset = Math.max(0, projection.offset - 200))}
        >Previous 200</button
      >
      <button
        disabled={projection.offset + projection.issues.length >=
          projection.count}
        onclick={() => (quality.selectedOffset = projection.offset + 200)}
        >Next 200</button
      >
    {/if}
  {:else}<p>{quality.error || 'Updating issues…'}</p>{/if}
  <button onclick={() => void onclose()}>Close</button>
</dialog>

<style>
  dialog {
    width: min(620px, 85vw);
    max-height: 80vh;
  }
  ol {
    max-height: 55vh;
    overflow: auto;
    padding-inline-start: 25px;
  }
  li {
    padding: 7px 0;
    border-bottom: 1px solid var(--line);
  }
  p {
    color: var(--text-muted);
  }
</style>
