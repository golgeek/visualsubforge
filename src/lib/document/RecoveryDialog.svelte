<script lang="ts">
  import { onMount } from 'svelte';
  import RecoveryCard from './RecoveryCard.svelte';
  import type { Recovery } from './recovery.svelte';
  import type { Workspace } from '../workspace/workspace.svelte';
  let {
    recovery,
    workspace,
    onclose,
    onchoose,
  }: {
    recovery: Recovery;
    workspace: Workspace;
    onclose: () => Promise<void>;
    onchoose: (id: string, copy: boolean) => Promise<void>;
  } = $props();
  let dialog: HTMLDialogElement;
  onMount(() => dialog.showModal());
</script>

<dialog
  bind:this={dialog}
  aria-label="Subtitle recovery"
  oncancel={(event) => {
    event.preventDefault();
    void onclose();
  }}
>
  <RecoveryCard
    {recovery}
    {workspace}
    {onchoose}
    disabled={recovery.active || !workspace.ready}
  />
  <button onclick={() => void onclose()}>Close recovery</button>
</dialog>

<style>
  dialog {
    width: min(720px, calc(100vw - 32px));
    padding: 16px;
  }
  button {
    margin-top: 12px;
  }
</style>
