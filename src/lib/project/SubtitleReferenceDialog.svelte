<script lang="ts">
  import { onMount } from 'svelte';
  import type {
    ReferenceChoice,
    ReferencePrompt,
  } from './project-workflow.svelte';
  let {
    prompt,
    onchoose,
  }: { prompt: ReferencePrompt; onchoose: (choice: ReferenceChoice) => void } =
    $props();
  let dialog: HTMLDialogElement;
  let cancel: HTMLButtonElement;
  const saving = $derived(prompt.action === 'save' || prompt.action === 'quit');
  const question = $derived.by(() => {
    switch (prompt.action) {
      case 'open':
        return `Open ${prompt.to}?`;
      case 'close':
        return 'Close the working subtitles?';
      case 'recover':
        return 'Open the recovered subtitles as an unsaved copy?';
      case 'extract':
        return 'Open the extracted subtitles as an unsaved copy?';
      case 'review':
        return `Start review in ${prompt.to}?`;
      case 'compare':
        return `Open ${prompt.to} as the revised subtitles for comparison?`;
      default:
        return prompt.to
          ? `Update the project to use ${prompt.to}?`
          : 'Remove the subtitle reference from the project?';
    }
  });
  onMount(() => {
    dialog.showModal();
    cancel.focus();
  });
</script>

<dialog
  bind:this={dialog}
  aria-label="Project subtitles"
  oncancel={(e) => {
    e.preventDefault();
    onchoose('cancel');
  }}
>
  <h2>{saving ? 'Update project subtitles?' : 'Change working subtitles?'}</h2>
  <p>
    {prompt.project}
    {prompt.from ? `uses ${prompt.from}.` : 'has no saved subtitle reference.'}
    {question}
  </p>
  {#if prompt.differentMedia}<p>
      Opening {prompt.differentMedia} will detach this project.
    </p>
  {:else if !prompt.canKeep}<p>
      Closing the only subtitle will detach this subtitle-only project.
    </p>
  {:else if !saving}<p>
      You can confirm the new reference the next time you save the project.
    </p>{/if}
  <div>
    <button bind:this={cancel} onclick={() => onchoose('cancel')}>Cancel</button
    >
    {#if prompt.canKeep}<button onclick={() => onchoose('keep')}
        >{saving ? 'Update project' : 'Use in project'}</button
      >{/if}
    {#if !saving}<button onclick={() => onchoose('detach')}
        >{prompt.action === 'close'
          ? 'Close without project'
          : 'Open without project'}</button
      >
    {:else if prompt.action === 'quit'}<button onclick={() => onchoose('quit')}
        >Quit without updating the project</button
      >{/if}
  </div>
</dialog>

<style>
  dialog {
    color: var(--text);
    background: var(--surface-panel);
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    padding: 20px;
    max-width: min(520px, calc(100vw - 24px));
    overflow-wrap: anywhere;
  }
  dialog::backdrop {
    background: var(--scrim);
  }
  h2 {
    margin-top: 0;
  }
  div {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
</style>
