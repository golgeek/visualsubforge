<script lang="ts">
  import { onMount } from 'svelte';
  let { onchoose }: { onchoose: (choice: 'save' | 'later') => void } = $props();
  let dialog: HTMLDialogElement;
  onMount(() => dialog.showModal());
</script>

<dialog
  bind:this={dialog}
  oncancel={(e) => {
    e.preventDefault();
    onchoose('later');
  }}
>
  <h2>Reopen this review later</h2>
  <p>
    The subtitle file contains only subtitles. Save a project to keep its
    original and comparison options.
  </p>
  <button onclick={() => onchoose('save')}>Save Project…</button><button
    onclick={() => onchoose('later')}>Later</button
  >
</dialog>
