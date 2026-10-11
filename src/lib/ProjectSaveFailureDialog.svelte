<script module lang="ts">
  export type ProjectExitChoice = 'retry' | 'saveAs' | 'cancel' | 'quit';
</script>

<script lang="ts">
  import { onMount } from 'svelte';
  let {
    error,
    onchoose,
  }: {
    error: string;
    onchoose: (choice: ProjectExitChoice) => void;
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
  aria-labelledby="project-save-failure-title"
  oncancel={(event) => {
    event.preventDefault();
    onchoose('cancel');
  }}
>
  <h2 id="project-save-failure-title">Project could not be saved</h2>
  <p class="error">{error}</p>
  <p>The project file has not been updated. Choose how to continue quitting.</p>
  <div class="actions">
    <button bind:this={cancel} onclick={() => onchoose('cancel')}
      >Cancel Quit</button
    >
    <button onclick={() => onchoose('retry')}>Retry</button>
    <button onclick={() => onchoose('saveAs')}>Save Project As…</button>
    <button onclick={() => onchoose('quit')}>Quit Without Saving Project</button
    >
  </div>
</dialog>

<style>
  dialog {
    color: #e7eef3;
    background: #1e303d;
    border: 1px solid #638299;
    border-radius: 8px;
    padding: 20px;
    width: min(480px, calc(100vw - 24px));
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
