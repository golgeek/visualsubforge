<script lang="ts">
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { WorkspaceDecision } from './workspace-decision.svelte';
  import type { ProjectReferenceIssue } from '../client';
  let {
    subtitles,
    subtitleDecision,
    projectExitDecision,
    projectReferenceDecision,
    confirmation = $bindable(),
  }: {
    subtitles: DocumentSession;
    subtitleDecision: WorkspaceDecision<
      'save' | 'discard' | 'cancel',
      'review'
    >;
    projectExitDecision: WorkspaceDecision<ProjectExitChoice, string>;
    projectReferenceDecision: WorkspaceDecision<
      'cancel' | 'choose' | 'reload',
      ProjectReferenceIssue
    >;
    confirmation: HTMLDialogElement | undefined;
  } = $props();
  import ProjectSaveFailureDialog, {
    type ProjectExitChoice,
  } from '../project/ProjectSaveFailureDialog.svelte';
  import ProjectReferenceDialog from '../project/ProjectReferenceDialog.svelte';
  const projectDirty = $derived(
    subtitleDecision.payload === 'review' ||
      subtitles.snapshot.translation?.dirty ||
      subtitles.snapshot.review?.dirty,
  );
  const doc = $derived(subtitles.snapshot);
</script>

<dialog
  bind:this={confirmation}
  oncancel={(e) => {
    e.preventDefault();
    subtitleDecision.answer('cancel');
  }}
  aria-label={projectDirty
    ? 'Unsaved project changes'
    : 'Unsaved subtitle changes'}
>
  <h2>
    {projectDirty ? 'Save project changes?' : 'Save subtitle changes?'}
  </h2>
  <p>
    {subtitleDecision.payload === 'review'
      ? 'Continuing removes the current review, including its discussion and export history. Save a project to keep it.'
      : doc.review?.dirty
        ? 'Save the review original and comparison options in a project.'
        : projectDirty
          ? 'Save your subtitle changes and the translation reference and offset in a project.'
          : 'Your current document or cue draft has changes.'}
  </p>
  <div class="dialog-actions">
    <button onclick={() => subtitleDecision.answer('cancel')}>Cancel</button
    ><button onclick={() => subtitleDecision.answer('discard')}
      >Discard changes</button
    ><button onclick={() => subtitleDecision.answer('save')}
      >{projectDirty ? 'Save project' : 'Save changes'}</button
    >
  </div>
</dialog>
{#if projectExitDecision.payload !== null}
  <ProjectSaveFailureDialog
    error={projectExitDecision.payload}
    onchoose={(choice) => void projectExitDecision.answer(choice)}
  />
{/if}
{#if projectReferenceDecision.payload !== null}
  <ProjectReferenceDialog
    issue={projectReferenceDecision.payload}
    onchoose={(choice) => void projectReferenceDecision.answer(choice)}
  />
{/if}

<style>
  dialog {
    color: var(--text);
    background: var(--surface-raised);
    border: 1px solid var(--accent);
    border-radius: 8px;
    max-width: 440px;
    padding: 24px;
  }

  dialog::backdrop {
    background: var(--scrim);
  }

  .dialog-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }

  h2 {
    margin-top: 0;
    font-size: 18px;
  }
</style>
