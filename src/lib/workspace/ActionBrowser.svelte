<script lang="ts">
  import { onMount } from 'svelte';
  import Kbd from '../ui/Kbd.svelte';
  import {
    actions,
    unavailable,
    type ActionId,
    type ActionContext,
  } from './action-registry';
  import type { Cue } from '../client';
  let {
    mode,
    context,
    cues,
    onaction,
    oncue,
    onclose,
  }: {
    mode: 'commands' | 'shortcuts' | 'actionMenu';
    context: ActionContext;
    cues: Cue[];
    onaction: (id: ActionId) => Promise<void>;
    oncue: (id: number) => Promise<void>;
    onclose: () => Promise<void>;
  } = $props();
  let dialog: HTMLDialogElement;
  let input: HTMLInputElement;
  let query = $state('');
  let pending = $state(false);
  const mac = navigator.platform.startsWith('Mac');
  const search = $derived(query.trim().toLocaleLowerCase());
  const filtered = $derived(
    actions.filter((action) =>
      [action.label, action.group, ...action.aliases].some((text) =>
        text.toLocaleLowerCase().includes(search),
      ),
    ),
  );
  const groups = $derived([...new Set(filtered.map((action) => action.group))]);
  const matches = $derived(
    mode === 'commands' && search
      ? cues
          .flatMap((cue, index) =>
            (
              search.startsWith('#')
                ? String(index + 1) === search.slice(1)
                : cue.text.toLocaleLowerCase().includes(search)
            )
              ? [{ cue, number: index + 1 }]
              : [],
          )
          .slice(0, 50)
      : [],
  );
  const title = $derived(
    mode === 'shortcuts'
      ? 'Keyboard shortcuts'
      : mode === 'actionMenu'
        ? 'Commands'
        : 'Command palette',
  );
  async function run(operation: () => Promise<void>) {
    if (pending) return;
    pending = true;
    try {
      await operation();
    } finally {
      pending = false;
    }
  }
  function keydown(event: KeyboardEvent) {
    if (
      event.key !== 'ArrowDown' &&
      event.key !== 'ArrowUp' &&
      event.key !== 'Enter'
    )
      return;
    const buttons = [
      ...dialog.querySelectorAll<HTMLButtonElement>(
        '[data-result]:not(:disabled)',
      ),
    ];
    if (!buttons.length) return;
    const current = buttons.indexOf(
      document.activeElement as HTMLButtonElement,
    );
    if (event.key === 'Enter') {
      if (document.activeElement !== input) return;
      event.preventDefault();
      buttons[0]!.click();
    } else {
      event.preventDefault();
      buttons[
        (current + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) %
          buttons.length
      ]!.focus();
    }
  }
  onMount(() => {
    dialog.showModal();
    input.focus();
  });
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="action-browser-title"
  onkeydown={keydown}
  oncancel={(event) => {
    event.preventDefault();
    void run(onclose);
  }}
>
  <header>
    <h2 id="action-browser-title">{title}</h2>
    <button disabled={pending} onclick={() => void run(onclose)}>Close</button>
  </header>
  <input
    bind:this={input}
    bind:value={query}
    aria-label={mode === 'shortcuts'
      ? 'Search shortcuts'
      : 'Search actions or cues'}
    placeholder={mode === 'commands'
      ? 'Search actions, cue text, or #21'
      : 'Search actions'}
  />
  <div class="results">
    {#if matches.length}<h3>Cues</h3>
      {#each matches as { cue, number } (cue.id)}
        <button
          data-result
          disabled={pending}
          onclick={() => void run(() => oncue(cue.id))}
          ><span>#{number} · {cue.text}</span></button
        >
      {/each}{/if}
    {#each groups as group (group)}
      <h3>{group}</h3>
      {#each filtered.filter((action) => action.group === group) as action (action.id)}
        {@const reason = unavailable(action.id, context)}
        {@const shortcut = mac
          ? (action.macShortcut ?? action.shortcut)
          : action.shortcut}
        {#if mode === 'shortcuts'}
          <div class="shortcut-row" data-action={action.id}>
            <span>{action.label}</span><span class="bindings"
              ><Kbd
                {shortcut}
              />{#each action.alternateShortcuts as alternative (alternative)}<Kbd
                  shortcut={alternative}
                />{/each}{#if !shortcut && !action.alternateShortcuts.length}<small
                  >Unassigned</small
                >{/if}</span
            >
          </div>
        {:else}
          <button
            data-result
            data-action={action.id}
            disabled={pending || !!reason}
            onclick={() => void run(() => onaction(action.id))}
            ><span
              >{action.label}{#if reason}<small>{reason}</small>{/if}</span
            ><Kbd {shortcut} /></button
          >
        {/if}
      {/each}
    {/each}
    {#if !filtered.length && !matches.length}<p>
        No matching actions or cues.
      </p>{/if}
  </div>
  {#if mode === 'shortcuts'}<p class="note">
      Plain Space stays text in the editor. Hold Space during playback in Timing
      mode to create a cue. Native menu shortcuts and text-input history retain
      their platform behavior.
    </p>{/if}
</dialog>

<style>
  dialog {
    width: min(580px, calc(100vw - 40px));
    max-height: min(680px, calc(100vh - 48px));
    padding: 20px;
  }
  header,
  .shortcut-row,
  .results button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }
  header {
    margin-bottom: 14px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  input {
    width: 100%;
  }
  .results {
    max-height: min(460px, calc(100vh - 240px));
    overflow: auto;
    margin-top: 12px;
  }
  h3 {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: uppercase;
    margin: 14px 8px 4px;
  }
  .results button {
    width: 100%;
    text-align: left;
    border-color: transparent;
    background: transparent;
    padding: 9px 8px;
  }
  .results button span {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .results button:hover:not(:disabled),
  .results button:focus-visible {
    background: var(--surface-hover);
  }
  small {
    display: block;
    color: var(--text-muted);
    font-size: 11px;
  }
  .shortcut-row {
    padding: 7px 8px;
  }
  .bindings {
    display: flex;
    gap: 8px;
  }
  .note {
    color: var(--text-muted);
    font-size: 12px;
    margin-bottom: 0;
  }
</style>
