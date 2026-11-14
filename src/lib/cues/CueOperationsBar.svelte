<script lang="ts">
  import type { Workspace } from '../workspace/workspace.svelte';
  import type { CueEditing } from './cue-editing.svelte';
  import type { WorkspaceLayout } from '../layout/workspace-layout.svelte';
  let {
    workspace,
    cueEditing,
    layout,
  }: { workspace: Workspace; cueEditing: CueEditing; layout: WorkspaceLayout } =
    $props();
  import { measurePaneControl } from '../layout/measure-pane-control';
</script>

<div
  class="cue-operationbar"
  aria-label="Cue operations"
  use:measurePaneControl={(height) => (layout.cueOperationsHeight = height)}
>
  {#each cueEditing.operations as op (op.operation)}
    <button
      disabled={!op.plan || workspace.busy || workspace.confirming}
      title={op.reason ||
        (op.operation === 'splitCue'
          ? 'Split timing and duplicate the accepted text in both halves.'
          : op.label)}
      onclick={() => void workspace.run(op.operation)}>{op.label}</button
    >
  {/each}
</div>

<style>
  .cue-operationbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 4px 8px;
    font-size: 11px;
    background: var(--surface-raised);
    flex-shrink: 0;
  }
</style>
