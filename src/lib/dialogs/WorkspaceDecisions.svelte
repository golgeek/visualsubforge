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
    subtitleDecision: WorkspaceDecision<'save' | 'discard' | 'cancel'>;
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
  const doc = $derived(subtitles.snapshot);
</script>

<dialog
  bind:this={confirmation}
  oncancel={(e) => {
    e.preventDefault();
    subtitleDecision.answer('cancel');
  }}
  aria-label={doc.translation?.dirty
    ? 'Unsaved project changes'
    : 'Unsaved subtitle changes'}
>
  <h2>
    {doc.translation?.dirty
      ? 'Save project changes?'
      : 'Save subtitle changes?'}
  </h2>
  <p>
    {doc.translation?.dirty
      ? 'Save your subtitle changes and the translation reference and offset in a project.'
      : 'Your current document or cue draft has changes.'}
  </p>
  <div class="dialog-actions">
    <button onclick={() => subtitleDecision.answer('cancel')}>Cancel</button
    ><button onclick={() => subtitleDecision.answer('discard')}
      >Discard changes</button
    ><button onclick={() => subtitleDecision.answer('save')}
      >{doc.translation?.dirty ? 'Save project' : 'Save changes'}</button
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
    color: #e7eef3;
    background: #203542;
    border: 1px solid #81ddc6;
    border-radius: 8px;
    max-width: 440px;
    padding: 24px;
  }

  dialog::backdrop {
    background: #0009;
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
