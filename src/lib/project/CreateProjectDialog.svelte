<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { chooseFile } from '../client';
  import type { CreateProjectPrompt } from './create-project';
  let {
    prompt,
    onclose,
  }: { prompt: CreateProjectPrompt; onclose: (created: boolean) => void } =
    $props();
  const initial = untrack(() => prompt);
  let projectName = $state(initial.projectName);
  let folder = $state(initial.folder);
  let companion = $state(initial.companionName);
  let customCompanion = $state(false);
  let saved = $state<string | null>(null);
  let error = $state('');
  let busy = $state(false);
  let dialog: HTMLDialogElement;
  let nameField: HTMLInputElement;
  onMount(() => {
    dialog.showModal();
    nameField.focus();
    nameField.select();
  });
  async function chooseFolder() {
    busy = true;
    try {
      folder = (await chooseFile('projectFolder', folder || null)) ?? folder;
    } catch (failure) {
      error = String(failure);
    } finally {
      busy = false;
    }
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    busy = true;
    error = '';
    try {
      const result = await prompt.submit({
        projectName: projectName.endsWith('.vsfproject')
          ? projectName
          : projectName + '.vsfproject',
        folder,
        companionName: saved ? null : companion,
      });
      if (result.kind === 'created') {
        onclose(true);
        return;
      }
      if (result.kind === 'subtitleSavedProjectFailed')
        saved = result.subtitlePath;
      error = result.error;
    } catch (failure) {
      error = String(failure);
    } finally {
      busy = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="create-project-title"
  oncancel={(event) => {
    event.preventDefault();
    if (!busy) onclose(false);
  }}
>
  <h2 id="create-project-title">Create Project</h2>
  <form onsubmit={submit}>
    <fieldset disabled={busy}>
      <label
        >Project name
        <input
          name="projectName"
          bind:this={nameField}
          bind:value={projectName}
          required
          oninput={(event) => {
            if (companion !== null && !customCompanion)
              companion =
                event.currentTarget.value.replace(/\.vsfproject$/, '') +
                prompt.suffix;
          }}
        />
      </label>
      <div class="destination">
        <label for="project-folder">Destination folder</label>
        <div class="folder">
          <input
            id="project-folder"
            name="folder"
            bind:value={folder}
            required
          />
          <button type="button" onclick={chooseFolder}>Choose…</button>
        </div>
      </div>
      {#if saved}<p role="status">Subtitle saved to {saved}</p>
      {:else if companion !== null}<label
          >Companion filename<input
            name="companionName"
            bind:value={companion}
            oninput={() => (customCompanion = true)}
            required
          /></label
        >
      {:else if prompt.reference}<p>References: {prompt.reference}</p>{/if}
      {#if error}<p role="alert">{error}</p>{/if}
      <div class="actions">
        <button type="button" onclick={() => onclose(false)}
          >{saved ? 'Close' : 'Cancel'}</button
        >
        <button type="submit" data-variant="primary"
          >{saved ? 'Retry project' : 'Create Project'}</button
        >
      </div>
    </fieldset>
  </form>
</dialog>

<style>
  dialog {
    width: min(440px, calc(100vw - 32px));
    max-height: calc(100dvh - 32px);
    overflow: auto;
    padding: 20px;
    border: 1px solid var(--line-strong);
    border-radius: 8px;
    color: var(--text);
    background: var(--surface-panel);
  }
  dialog::backdrop {
    background: var(--scrim);
  }
  h2 {
    margin: 0 0 16px;
  }
  fieldset {
    display: grid;
    gap: 14px;
    min-width: 0;
    padding: 0;
    border: 0;
  }
  label,
  .destination {
    display: grid;
    gap: 4px;
  }
  .folder,
  .actions {
    display: flex;
    gap: 8px;
  }
  .folder input {
    flex: 1;
    min-width: 0;
  }
  .actions {
    justify-content: flex-end;
  }
  p {
    margin: 0;
    overflow-wrap: anywhere;
  }
  [role='alert'] {
    color: var(--err);
  }
</style>
