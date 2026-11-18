<script lang="ts">
  import { onMount } from 'svelte';
  import { languageName } from './language';
  import type { ExtractionSession } from './extraction-session.svelte';
  let { session }: { session: ExtractionSession } = $props();
  let dialog: HTMLDialogElement;
  const labels: Record<string, string> = {
    subrip: 'SRT',
    ass: 'ASS',
    ssa: 'SSA',
    webvtt: 'WebVTT',
    mov_text: 'MP4 text',
  };
  onMount(() => dialog.showModal());
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="extraction-title"
  oncancel={(event) => {
    event.preventDefault();
    void session.cancel();
  }}
>
  <h2 id="extraction-title">Open Subtitles from Video</h2>
  <p>{session.path.split(/[\\/]/).pop()}</p>
  {#if session.error}<p role="alert">{session.error}</p>{/if}
  {#if session.busy}<p role="status">
      {session.extracting === null
        ? 'Reading subtitle tracks…'
        : `Extracting track ${session.extracting}…`}
      {session.elapsed}s
    </p>{/if}
  {#if session.view?.listing}
    {#if !session.view.listing.tracks.length}<p>
        This video has no subtitle tracks.
      </p>{/if}
    {#each session.view.listing.tracks as track (track.index)}
      <label class="track">
        <input
          type="radio"
          name="track"
          value={track.index}
          bind:group={session.selected}
          disabled={session.busy || !!track.unavailable}
        />
        <span
          >Track {track.index}: {languageName(track.language)}{track.language
            ? ` (${track.language})`
            : ''}{track.title ? ` — ${track.title}` : ''}
          {labels[track.codec] ?? track.codec}
          {#if track.default}<small>Default</small>{/if}
          {#if track.forced}<small>Forced</small>{/if}
          {#if track.hearing_impaired}<small>Hearing impaired (SDH)</small>{/if}
          {#if track.unavailable}<span>{track.unavailable}</span>{/if}
        </span>
      </label>
    {/each}
  {/if}
  <footer>
    <button onclick={() => void session.cancel()}>Cancel</button><button
      disabled={session.busy || session.selected === null}
      onclick={() => void session.extract()}>Extract</button
    >
  </footer>
</dialog>

<style>
  dialog {
    max-width: min(48rem, 90vw);
  }
  .track {
    display: flex;
    gap: 0.6rem;
    padding: 0.5rem;
  }
  small {
    margin-left: 0.5rem;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 1rem;
  }
</style>
