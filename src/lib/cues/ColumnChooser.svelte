<script lang="ts">
  import { onMount } from 'svelte';
  import type { WorkspaceLayout } from '../layout/workspace-layout.svelte';
  import {
    availableColumn,
    columnLabels,
    mandatoryColumn,
    moveColumn,
    showColumn,
    type ColumnId,
  } from './cue-columns';
  let {
    layout,
    styled,
    onclose,
  }: {
    layout: WorkspaceLayout;
    styled: boolean;
    onclose: () => Promise<void>;
  } = $props();
  let dialog: HTMLDialogElement;
  let dragged = $state<ColumnId | null>(null);
  const columns = $derived(
    layout.columns.items.filter((c) => availableColumn(c.id, styled)),
  );
  onMount(() => dialog.showModal());
  function move(id: ColumnId, target: ColumnId) {
    layout.setColumns(moveColumn(layout.columns, id, target));
  }
</script>

<dialog
  bind:this={dialog}
  aria-label="Cue columns"
  oncancel={(e) => {
    e.preventDefault();
    void onclose();
  }}
>
  <h2>Columns</h2>
  <p>Saved with this layout. Issues and Text stay visible.</p>
  <ol>
    {#each columns as column, index (column.id)}
      <li
        draggable={column.id !== 'issue'}
        data-column={column.id}
        ondragstart={(e) => {
          dragged = column.id;
          e.dataTransfer?.setData('text/plain', column.id);
        }}
        ondragend={() => (dragged = null)}
        ondragover={(e) => {
          if (column.id !== 'issue') e.preventDefault();
        }}
        ondrop={(e) => {
          e.preventDefault();
          if (dragged) move(dragged, column.id);
          dragged = null;
        }}
      >
        <span aria-hidden="true">⠿</span>
        <label
          ><input
            type="checkbox"
            checked={column.visible}
            disabled={mandatoryColumn(column.id)}
            onchange={(e) =>
              layout.setColumns(
                showColumn(layout.columns, column.id, e.currentTarget.checked),
              )}
          />{columnLabels[column.id]}</label
        >
        <button
          aria-label={`Move ${columnLabels[column.id]} column up`}
          disabled={index < 2}
          onclick={() => move(column.id, columns[index - 1]!.id)}>↑</button
        >
        <button
          aria-label={`Move ${columnLabels[column.id]} column down`}
          disabled={column.id === 'issue' || index === columns.length - 1}
          onclick={() => move(column.id, columns[index + 1]!.id)}>↓</button
        >
      </li>
    {/each}
  </ol>
  <fieldset>
    <legend>Row density</legend>
    {#each ['compact', 'comfortable'] as density (density)}<label
        ><input
          type="radio"
          name="density"
          value={density}
          checked={layout.columns.density === density}
          onchange={() =>
            layout.setColumns({
              ...layout.columns,
              density: density as 'compact' | 'comfortable',
            })}
        />{density === 'compact'
          ? 'Compact · 24 px'
          : 'Comfortable · 32 px'}</label
      >{/each}
  </fieldset>
  <p>Drag a header edge to resize. Double-click it to fit visible rows.</p>
  <button onclick={() => void onclose()}>Done</button>
</dialog>

<style>
  dialog {
    width: min(440px, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    overflow: auto;
    padding: 18px;
    background: var(--surface-panel);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
  }
  dialog::backdrop {
    background: var(--scrim);
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  p {
    color: var(--text-muted);
    font-size: 12px;
  }
  ol {
    list-style: none;
    padding: 0;
    margin: 12px 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 4px;
  }
  li:hover {
    background: var(--surface-hover);
  }
  li > span {
    color: var(--text-muted);
    cursor: grab;
  }
  label {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  li label {
    flex: 1;
  }
  li button {
    padding: 0 7px;
  }
  fieldset {
    display: flex;
    gap: 14px;
    border: 1px solid var(--line);
    padding: 8px;
  }
  legend {
    color: var(--text-muted);
    font-size: 12px;
  }
</style>
