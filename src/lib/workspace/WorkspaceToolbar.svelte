<script lang="ts">
  import type { Workspace } from './workspace.svelte';
  import type { MediaSession } from '../media/media-session.svelte';
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { ProjectWorkflow } from '../project/project-workflow.svelte';
  let {
    workspace,
    media,
    subtitles,
    projects,
    nativeMenus,
    diagnostics,
    showScenes = $bindable(),
    sceneState,
    oncommands,
    onshortcuts,
  }: {
    workspace: Workspace;
    media: MediaSession;
    subtitles: DocumentSession;
    projects: ProjectWorkflow;
    nativeMenus: boolean;
    diagnostics: boolean;
    showScenes: boolean;
    oncommands: () => void;
    onshortcuts: () => void;
    sceneState: { pending: boolean; running: boolean };
  } = $props();
  const doc = $derived(subtitles.snapshot);
  const status = $derived(media.status);
  const draftDirty = $derived(subtitles.dirty);
</script>

<header class="toolbar">
  <strong>VisualSubForge</strong>
  <button disabled={!workspace.canAct} onclick={oncommands}>Commands</button>
  <button
    disabled={!workspace.canAct}
    onclick={onshortcuts}
    aria-label="Keyboard shortcuts">?</button
  >
  {#if !nativeMenus}
    <button
      disabled={workspace.busy || !workspace.ready}
      onclick={() => void workspace.run('openProject')}>Open Project</button
    >
    <button
      disabled={!workspace.canAct}
      onclick={() => void workspace.run('importLegacyProject')}
      >Import VisualSubSync Project</button
    >
    <button
      disabled={workspace.busy ||
        !workspace.ready ||
        (!status.loaded && !doc.loaded)}
      title={projects.path ??
        'Save subtitle references, media, scene results and workspace options'}
      onclick={() => void workspace.run('saveProject')}>Save Project</button
    >
    <button
      disabled={workspace.busy || !workspace.ready || !status.available}
      onclick={() => void workspace.run('openMedia')}>Open Media</button
    >
    <button
      disabled={workspace.busy || !workspace.ready}
      onclick={() => void workspace.run('openSubtitles')}>Open Subtitles</button
    >
    <button
      disabled={workspace.busy || !doc.loaded}
      onclick={() => void workspace.run('save')}>Save</button
    >
  {/if}
  <span class="filename" title={doc.path ?? ''}
    >{doc.path?.split(/[\\/]/).pop() ??
      (doc.loaded ? 'Untitled subtitles' : 'No subtitles')}{doc.dirty ||
    draftDirty
      ? ' •'
      : ''}{doc.requiresSaveAs ? ' (Save As required)' : ''}</span
  >
  {#if !nativeMenus}
    <button
      class="quiet"
      onclick={() => void workspace.run('diagnostics')}
      aria-pressed={diagnostics}>Diagnostics</button
    >
    <button
      disabled={!workspace.canAct}
      onclick={() => void workspace.run('settings')}>Settings…</button
    >
    <button
      disabled={workspace.busy ||
        workspace.confirming ||
        !workspace.ready ||
        !status.loaded ||
        !doc.cues.length}
      onclick={() => void workspace.run('resync')}
      >Resynchronize subtitles…</button
    >
    <button
      disabled={!status.loaded ||
        workspace.busy ||
        workspace.confirming ||
        sceneState.pending}
      onclick={() =>
        void workspace.run(
          sceneState.running ? 'cancelSceneScan' : 'scanScenes',
        )}
      >{sceneState.running ? 'Cancel scene scan' : 'Scan scene changes'}</button
    >
    <label
      ><input type="checkbox" bind:checked={showScenes} /> Show scene changes</label
    >
  {/if}
</header>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    background: var(--surface-panel);
    z-index: 2;
    border-bottom: 1px solid var(--line);
  }
  .toolbar strong {
    margin-right: 8px;
  }
  .quiet {
    background: transparent;
  }
  @media (max-width: 799px), (max-height: 609px) {
    .toolbar {
      flex-wrap: wrap;
      min-height: 48px;
      padding: 8px;
    }
    .toolbar strong {
      flex-basis: 100%;
    }
  }
  .filename {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
    min-width: 0;
    color: var(--text-muted);
  }
  @media (max-width: 799px), (max-height: 609px) {
    .filename {
      flex-basis: 100%;
    }
  }
</style>
