<script lang="ts">
  import Tooltip from '../ui/Tooltip.svelte';
  import type { Workspace } from '../workspace/workspace.svelte';
  import type { CueEditing } from './cue-editing.svelte';
  let {
    workspace,
    cueEditing,
  }: { workspace: Workspace; cueEditing: CueEditing } = $props();
  const paths: Record<string, string> = {
    createCue: 'M8 2v12M2 8h12',
    deleteCue: 'M3 4h10M6 4V2h4v2M4 4l1 10h6l1-10',
    splitCue: 'M8 1v14M2 4l3 4-3 4M14 4l-3 4 3 4',
    mergePrevious: 'M7 3L2 8l5 5M2 8h12',
    mergeNext: 'M9 3l5 5-5 5M14 8H2',
  };
</script>

<div class="cue-operationbar" aria-label="Cue operations">
  {#each cueEditing.operations as op (op.operation)}
    <Tooltip action={op.operation}
      ><button
        aria-describedby="workspace-tooltip"
        aria-label={op.label}
        disabled={!op.plan || workspace.busy || workspace.confirming}
        onclick={() => void workspace.run(op.operation)}
        ><svg aria-hidden="true" width="16" height="16" viewBox="0 0 16 16"
          ><path
            d={paths[op.operation]}
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
          /></svg
        ><span class="sr-only">{op.label}</span></button
      ></Tooltip
    >
  {/each}
</div>

<style>
  .cue-operationbar {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 0;
    font-size: 11px;
    background: var(--surface-raised);
    flex-shrink: 0;
  }
  button {
    width: 27px;
    height: 27px;
    padding: 4px;
    display: grid;
    place-items: center;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
