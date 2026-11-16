<script lang="ts">
  import { onMount } from 'svelte';
  let {
    path,
    onchoose,
  }: { path: string; onchoose: (choice: 'swap' | 'keep' | 'cancel') => void } =
    $props();
  let dialog: HTMLDialogElement;
  onMount(() => dialog.showModal());
</script>

<dialog
  bind:this={dialog}
  oncancel={(e) => {
    e.preventDefault();
    onchoose('cancel');
  }}
>
  <p>
    {path.split(/[\\/]/u).at(-1)} is newer than the open file. Compare the other way
    around?
  </p>
  <button onclick={() => onchoose('swap')}>Swap</button><button
    onclick={() => onchoose('keep')}>Keep</button
  ><button onclick={() => onchoose('cancel')}>Cancel</button>
</dialog>
