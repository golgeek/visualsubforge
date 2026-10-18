<script lang="ts">
  import { onMount } from 'svelte';
  import type { ProjectReferenceIssue } from './client';
  let {
    issue,
    onchoose,
  }: {
    issue: ProjectReferenceIssue;
    onchoose: (choice: 'cancel' | 'choose' | 'reload') => void;
  } = $props();
  let dialog: HTMLDialogElement;
  let cancel: HTMLButtonElement;
  onMount(() => {
    dialog.showModal();
    cancel.focus();
  });
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="project-reference-title"
  oncancel={(event) => {
    event.preventDefault();
    onchoose('cancel');
  }}
>
  <h2 id="project-reference-title">
    {issue.canReload ? 'Reference file changed' : 'Reference file unavailable'}
  </h2>
  <p>{issue.path}</p>
  <p class="error">{issue.message}</p>
  <p>Your current subtitles and cue draft are still open.</p>
  <div class="actions">
    <button bind:this={cancel} onclick={() => onchoose('cancel')}
      >Cancel Open</button
    >
    <button onclick={() => onchoose('choose')}>Choose Reference…</button>
    {#if issue.canReload}<button onclick={() => onchoose('reload')}
        >Reload Changed Reference</button
      >{/if}
  </div>
</dialog>

<style>
  dialog {
    color: #e7eef3;
    background: #1e303d;
    border: 1px solid #638299;
    border-radius: 8px;
    padding: 20px;
    width: min(520px, calc(100vw - 24px));
    max-height: calc(100dvh - 24px);
    overflow: auto;
  }
  dialog::backdrop {
    background: #0009;
  }
  h2 {
    margin-top: 0;
  }
  p {
    overflow-wrap: anywhere;
  }
  .error {
    color: #ffc4b7;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: flex-end;
  }
</style>
