<script lang="ts">
  import type { WindowChrome } from './window-chrome.svelte';
  import type { ActionGroup } from './action-registry';
  const groups: ActionGroup[] = [
    'File',
    'Edit',
    'Cue',
    'View',
    'Playback',
    'Tools',
    'Review',
    'Help',
  ];
  import type { Workspace } from './workspace.svelte';
  import type { MediaSession } from '../media/media-session.svelte';
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { ProjectWorkflow } from '../project/project-workflow.svelte';
  let {
    workspace,
    chrome,
    ongroup,
    media,
    subtitles,
    projects,
    nativeMenus,
    diagnostics,
    showScenes = $bindable(),
    sceneState,
    oncommands,
    onshortcuts,
    onlayout,
    onrecovery,
    recoveryAvailable = false,
  }: {
    workspace: Workspace;
    chrome?: WindowChrome;
    ongroup?: (group: ActionGroup) => void;
    media: MediaSession;
    subtitles: DocumentSession;
    projects: Pick<ProjectWorkflow, 'path'>;
    nativeMenus: boolean;
    diagnostics: boolean;
    showScenes: boolean;
    oncommands: () => void;
    onshortcuts: () => void;
    onlayout: () => void;
    onrecovery?: () => void;
    recoveryAvailable?: boolean;
    sceneState: { pending: boolean; running: boolean };
  } = $props();
  const doc = $derived(subtitles.snapshot);
  const status = $derived(media.status);
  const draftDirty = $derived(subtitles.dirty);
</script>

<header
  class="toolbar"
  class:mac-overlay={chrome?.overlay && chrome.platform === 'macos'}
>
  <strong>{chrome?.menus ? 'VSF' : 'VisualSubForge'}</strong>
  {#if chrome?.menus}
    <nav aria-label="Application menus">
      {#each groups as group (group)}
        <button
          disabled={!workspace.canAct}
          aria-haspopup="dialog"
          onclick={() => ongroup?.(group)}>{group}</button
        >
      {/each}
    </nav>
  {/if}
  {#if recoveryAvailable}<button
      disabled={!workspace.canAct}
      onclick={onrecovery}>Recovery</button
    >{/if}
  <button disabled={!workspace.canAct} onclick={onlayout}>Layout</button>
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
  {#snippet filename()}
    {doc.path?.split(/[\\/]/).pop() ??
      (doc.loaded ? 'Untitled subtitles' : 'No subtitles')}{doc.dirty ||
    draftDirty
      ? ' •'
      : ''}{doc.requiresSaveAs ? ' (Save As required)' : ''}
  {/snippet}
  {#if chrome?.overlay}
    <button
      class="filename drag-region"
      data-window-drag
      aria-label="Move window"
      title={`${doc.path ?? 'VisualSubForge'} · Drag to move; double-click to maximize or restore`}
      onmousedown={(event) => void chrome?.drag(event)}
      ondblclick={() => void chrome?.maximize()}>{@render filename()}</button
    >
  {:else}
    <span class="filename" title={doc.path ?? ''}>{@render filename()}</span>
  {/if}
  {#if chrome?.custom}
    <div class="captions" aria-label="Window controls">
      <button
        aria-label="Minimize window"
        onclick={() => void chrome?.minimize()}>—</button
      >
      <button
        aria-label="Maximize or restore window"
        onclick={() => void chrome?.maximize()}>□</button
      >
      <button
        aria-label="Close window"
        disabled={!workspace.canAct}
        onclick={() => void workspace.run('quit')}>×</button
      >
    </div>
  {/if}
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
  .mac-overlay {
    padding-left: 88px;
  }
  nav,
  .captions {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
  }
  nav button {
    background: transparent;
    border-color: transparent;
    padding: 4px 5px;
  }
  .captions button {
    width: 32px;
    padding: 4px;
  }
  .drag-region {
    min-width: 40px;
    text-align: left;
    background: transparent;
    border-color: transparent;
    cursor: move;
    color: var(--text-muted);
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
    .mac-overlay {
      padding-left: 88px;
    }
    nav,
    .captions {
      display: flex;
      gap: 2px;
      flex-shrink: 0;
    }
    nav button {
      background: transparent;
      border-color: transparent;
      padding: 4px 5px;
    }
    .captions button {
      width: 32px;
      padding: 4px;
    }
    .drag-region {
      min-width: 40px;
      text-align: left;
      background: transparent;
      border-color: transparent;
      cursor: move;
      color: var(--text-muted);
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
