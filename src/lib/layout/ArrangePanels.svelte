<script lang="ts">
  import { onMount } from 'svelte';
  import type { WorkspaceLayout } from './workspace-layout.svelte';
  import {
    movePanel,
    panelNames,
    panels,
    type Panel,
    type Move,
  } from './arrangement';
  let {
    layout,
    projectPath,
    onclose,
    onsave,
  }: {
    layout: WorkspaceLayout;
    projectPath: string | null;
    onclose: () => Promise<void>;
    onsave: () => Promise<boolean>;
  } = $props();
  let dialog: HTMLDialogElement;
  let selected = $state<Panel>('waveform');
  let kind = $state<'swap' | 'before' | 'after' | 'beside'>('swap');
  let pending = $state(false);
  let error = $state('');
  let notice = $state('');
  const shown = $derived(layout.arrangement.rows.flatMap((row) => row.panels));
  onMount(() => dialog.showModal());
  async function run(action: () => Promise<unknown>) {
    if (pending) return;
    pending = true;
    error = '';
    notice = '';
    try {
      await action();
    } catch (failure) {
      error = String(failure);
    } finally {
      pending = false;
    }
  }
  function move(action: Move) {
    return run(() =>
      layout.arrange(movePanel($state.snapshot(layout.arrangement), action)),
    );
  }
  function resize(row: number, field: 'weight' | 'split', value: number) {
    if (!Number.isInteger(value)) return;
    const next = structuredClone($state.snapshot(layout.arrangement));
    next.rows[row]![field] = value;
    void run(() => layout.arrange(next));
  }
</script>

<dialog
  bind:this={dialog}
  aria-label="Arrange panels"
  oncancel={(event) => {
    event.preventDefault();
    if (!pending) void onclose();
  }}
>
  <header>
    <h2>Arrange panels</h2>
    <button disabled={pending} onclick={() => void onclose()}>Done</button>
  </header>
  <p>
    Video is paused and hidden while you arrange. Drag a panel onto another, or
    select a panel and destination below.
  </p>
  <div class="controls">
    <label
      >Panel <select bind:value={selected} disabled={pending}
        >{#each panels as panel (panel)}<option value={panel}
            >{panelNames[panel]}</option
          >{/each}</select
      ></label
    >
    <label
      >Drop action <select bind:value={kind} disabled={pending}>
        <option value="swap">Swap / replace</option><option value="before"
          >Insert row before</option
        ><option value="after">Insert row after</option><option value="beside"
          >Place beside</option
        >
      </select></label
    >
    <button
      disabled={pending || !shown.includes(selected)}
      onclick={() => void move({ panel: selected, kind: 'hide' })}
      >Hide selected panel</button
    >
  </div>
  <div class="schematic" aria-label="Panel slots">
    {#each layout.arrangement.rows as row, index (index)}
      <div class="row">
        <div class="tiles">
          {#each row.panels as panel (panel)}
            <button
              draggable={!pending}
              disabled={pending}
              aria-label={`Place ${panelNames[selected]} at ${panelNames[panel]}`}
              ondragstart={(event) => {
                selected = panel;
                event.dataTransfer?.setData('text/plain', panel);
              }}
              ondragover={(event) => event.preventDefault()}
              ondrop={(event) => {
                event.preventDefault();
                if (
                  panels.includes(
                    event.dataTransfer?.getData('text/plain') as Panel,
                  )
                )
                  void move({
                    panel: event.dataTransfer!.getData('text/plain') as Panel,
                    target: panel,
                    kind,
                  });
              }}
              onclick={() =>
                void move({ panel: selected, target: panel, kind })}
              >{panelNames[panel]}</button
            >
          {/each}
        </div>
        <label
          >Height weight <input
            type="number"
            min="1"
            max="100"
            value={row.weight}
            disabled={pending}
            onchange={(event) => {
              if (event.currentTarget.validity.valid)
                resize(index, 'weight', event.currentTarget.valueAsNumber);
            }}
          /></label
        >
        {#if row.panels.length === 2}<label
            >Left width % <input
              type="number"
              min="20"
              max="80"
              value={row.split}
              disabled={pending}
              onchange={(event) => {
                if (event.currentTarget.validity.valid)
                  resize(index, 'split', event.currentTarget.valueAsNumber);
              }}
            /></label
          >{/if}
      </div>
    {/each}
  </div>
  <div class="hidden">
    <strong>Hidden panels</strong
    >{#each panels.filter((panel) => !shown.includes(panel)) as panel (panel)}<button
        draggable={!pending}
        disabled={pending}
        ondragstart={(event) => {
          selected = panel;
          event.dataTransfer?.setData('text/plain', panel);
        }}
        onclick={() => (selected = panel)}>{panelNames[panel]}</button
      >{/each}
  </div>
  <p class="hint">
    Review includes Statistics. Two panels per row; invalid sizes are refused.
    Smaller windows scroll to preserve controls. Reset returns to the selected
    standard preset.
  </p>
  <label class="remember"
    ><input
      type="checkbox"
      checked={layout.remembered}
      disabled={pending || !projectPath}
      onchange={(event) => {
        const value = event.currentTarget.checked;
        void run(() => layout.rememberForProject(value));
      }}
    />Remember for this project</label
  >
  <p class="hint">
    {projectPath
      ? layout.remembered
        ? 'Project layout changes stay in this workspace until Save project layout or Save Project. Turning this off restores your user layout; save to remove the project override.'
        : 'Changes save as your user default. Save project layout also removes a previously saved project override.'
      : 'Save a project first to enable a project-specific layout.'}
  </p>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if notice}<p role="status">{notice}</p>{/if}
  <footer>
    <button disabled={pending} onclick={() => void run(() => layout.reset())}
      >Reset arrangement</button
    ><button
      disabled={pending || !projectPath}
      onclick={() =>
        void run(async () => {
          if (await onsave())
            notice = 'Project layout saved. Subtitle bytes were not saved.';
        })}>Save project layout</button
    >
  </footer>
</dialog>

<style>
  dialog {
    width: min(900px, calc(100vw - 32px));
    max-height: calc(100dvh - 32px);
    overflow: auto;
    padding: 20px;
    color: var(--text);
    background: var(--surface-panel);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
  }
  dialog::backdrop {
    background: var(--scrim);
  }
  header,
  footer,
  .controls,
  .hidden {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  header {
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  p {
    color: var(--text-muted);
  }
  .hint {
    font-size: 12px;
  }
  [role='alert'] {
    color: var(--err);
  }
  label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .schematic {
    display: grid;
    gap: 6px;
    margin: 16px 0;
  }
  .row {
    display: flex;
    gap: 8px;
    padding: 8px;
    background: var(--surface-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius-md);
  }
  .tiles {
    display: flex;
    flex: 1;
    gap: 6px;
  }
  .tiles button {
    flex: 1;
    min-height: 42px;
    background: var(--selection);
    border: 1px solid var(--accent);
  }
  input[type='number'] {
    width: 56px;
  }
  .hidden {
    margin: 12px 0;
  }
  footer {
    margin-top: 12px;
  }
</style>
