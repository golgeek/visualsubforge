<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { DocumentSession } from './document-session.svelte';
  import type { MediaStatus } from './client';
  import { ResyncSession } from './resync-session.svelte';
  import { formatTime } from './timeline';

  let {
    subtitles,
    media,
    target,
    onclose,
    onaccepted,
  }: {
    subtitles: DocumentSession;
    media: () => MediaStatus;
    target: string;
    onclose: () => void;
    onaccepted: () => void;
  } = $props();
  const session = untrack(() => new ResyncSession(subtitles, media));
  let dialog: HTMLDialogElement;
  let choose: HTMLButtonElement;
  const view = $derived(session.view);
  const timeUs = (value: number) => formatTime(Math.round(value / 1000));
  onMount(() => {
    dialog.showModal();
    choose.focus();
    const timer = setInterval(() => void session.poll(), 250);
    return () => {
      clearInterval(timer);
      session.dispose();
    };
  });
  async function accept() {
    if (await session.accept()) onaccepted();
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="resync-title"
  oncancel={(event) => {
    event.preventDefault();
    if (session.canClose) onclose();
  }}
>
  <h2 id="resync-title">Resynchronize subtitles</h2>
  <p>
    Transfer the current subtitles from their original video to the loaded
    target video.
  </p>
  <dl>
    <dt>Source video</dt>
    <dd>
      {session.source || 'Choose the video these subtitles were timed for.'}
      <button
        bind:this={choose}
        disabled={!session.canStart}
        onclick={() => void session.chooseSource()}>Choose source video…</button
      >
    </dd>
    <dt>Source-timed subtitles</dt>
    <dd>
      {subtitles.snapshot.path ?? 'Untitled subtitles'} · {subtitles.snapshot
        .cues.length} cues{#if subtitles.snapshot.dirty}
        · unsaved changes{/if}
    </dd>
    <dt>Target video</dt>
    <dd>{target}</dd>
  </dl>
  {#if subtitles.dirty}
    <section class="draft" aria-label="Unapplied cue draft">
      <p>
        The current cue has an unapplied draft. Apply or discard it before
        analysis.
      </p>
      <button
        disabled={!session.canStart}
        onclick={() => void session.applyDraft()}>Apply cue draft</button
      >
      <button disabled={!session.canStart} onclick={() => subtitles.revert()}
        >Discard cue draft</button
      >
      <button disabled={!session.canClose} onclick={onclose}
        >Cancel resync</button
      >
    </section>
  {/if}
  <div class="actions start">
    <button
      disabled={!session.canStart ||
        !session.source ||
        subtitles.dirty ||
        !media().loaded ||
        !subtitles.snapshot.cues.length}
      onclick={() => void session.start()}
    >
      {view ? 'Analyze again' : 'Analyze videos'}
    </button>
    {#if session.running || session.accepting || session.cancelling}
      <button
        disabled={!session.canCancel}
        onclick={() => void session.cancel()}>Cancel operation</button
      >
    {/if}
  </div>
  <p role="status">
    {#if session.accepting}Checking inputs and applying timings…
    {:else if session.cancelling || view?.phase === 'cancelling'}Cancelling and
      cleaning up analysis…
    {:else if session.pending}Working…
    {:else if view?.phase === 'analyzing'}{view.stage}…
    {:else if view?.phase === 'cancelled'}Analysis cancelled. Your subtitles are
      unchanged.
    {:else if view?.phase === 'reviewing'}{view.totalCues - view.unresolvedCues} of
      {view.totalCues} cues mapped · {view.totalSections} matched section{view.totalSections ===
      1
        ? ''
        : 's'}
    {/if}
  </p>
  {#if session.error}<p class="error" role="alert">{session.error}</p>{/if}
  {#if view?.phase === 'reviewing'}
    {#if !view.complete}
      <p class="error">
        {view.unresolvedCues} cues could not be mapped safely. This proposal cannot
        be accepted; no timings have changed.
      </p>
    {/if}
    <section aria-label="Matched sections">
      <h3>Matched sections</h3>
      <p>
        Timing scale shows how long one source second becomes in the target.
        Separate sections keep ad and edit gaps separate.
      </p>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable tables need keyboard focus for arrow/Page navigation.) -->
      <div
        class="table-scroll"
        tabindex="0"
        role="region"
        aria-label="Matched section details"
      >
        <table>
          <thead
            ><tr
              ><th>Source interval</th><th>Target interval</th><th
                >Timing scale</th
              ><th>Evidence</th></tr
            ></thead
          >
          <tbody
            >{#each view.sections as section, index (view.sectionOffset + index)}
              <tr
                ><td
                  >{timeUs(section.sourceStartUs)} → {timeUs(
                    section.sourceEndUs,
                  )}</td
                >
                <td
                  >{timeUs(section.targetStartUs)} → {timeUs(
                    section.targetEndUs,
                  )}</td
                >
                <td
                  >{(
                    (section.targetEndUs - section.targetStartUs) /
                    (section.sourceEndUs - section.sourceStartUs)
                  ).toFixed(6)}×</td
                >
                <td
                  >{section.anchors} anchors · {(
                    section.maxResidualUs / 1000
                  ).toFixed(2)} ms maximum fit error</td
                ></tr
              >
            {/each}</tbody
          >
        </table>
      </div>
      {#if view.totalSections > 100}
        <nav aria-label="Matched section pages">
          <button
            disabled={!session.canStart || view.sectionOffset === 0}
            onclick={() =>
              void session.page(
                view!.cueOffset,
                Math.max(0, view!.sectionOffset - 100),
              )}>Previous sections</button
          >
          <span
            >{view.sectionOffset + 1}–{view.sectionOffset +
              view.sections.length} of {view.totalSections}</span
          >
          <button
            disabled={!session.canStart ||
              view.sectionOffset + view.sections.length >= view.totalSections}
            onclick={() =>
              void session.page(view!.cueOffset, view!.sectionOffset + 100)}
            >Next sections</button
          >
        </nav>
      {/if}
    </section>
    <section aria-label="Proposed subtitle timings">
      <h3>Proposed timings</h3>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable tables need keyboard focus for arrow/Page navigation.) -->
      <div
        class="table-scroll"
        tabindex="0"
        role="region"
        aria-label="Cue timing details"
      >
        <table>
          <thead
            ><tr
              ><th>Cue</th><th>Text</th><th>Original timing</th><th
                >Proposed timing</th
              ></tr
            ></thead
          >
          <tbody
            >{#each view.cues as cue, index (cue.id)}
              <tr class:unresolved={cue.issue !== null}
                ><td>{view.cueOffset + index + 1}</td><td class="cue-text"
                  >{cue.text}</td
                >
                <td
                  >{formatTime(cue.sourceStartMs)} → {formatTime(
                    cue.sourceEndMs,
                  )}</td
                >
                <td
                  >{#if cue.startMs !== null && cue.endMs !== null}{formatTime(
                      cue.startMs,
                    )} → {formatTime(cue.endMs)}{:else}{cue.issue}{/if}</td
                ></tr
              >
            {/each}</tbody
          >
        </table>
      </div>
      {#if view.totalCues > 100}
        <nav aria-label="Cue pages">
          <button
            disabled={!session.canStart || view.cueOffset === 0}
            onclick={() =>
              void session.page(
                Math.max(0, view!.cueOffset - 100),
                view!.sectionOffset,
              )}>Previous cues</button
          >
          <span
            >{view.cueOffset + 1}–{view.cueOffset + view.cues.length} of {view.totalCues}</span
          >
          <button
            disabled={!session.canStart ||
              view.cueOffset + view.cues.length >= view.totalCues}
            onclick={() =>
              void session.page(view!.cueOffset + 100, view!.sectionOffset)}
            >Next cues</button
          >
        </nav>
      {/if}
    </section>
  {/if}
  <p>
    You can undo timing changes in one step. Save As to a different file is
    required, even after Undo. Video previews keep the accepted subtitles until
    you accept.
  </p>
  <div class="actions">
    <button disabled={!session.canClose} onclick={onclose}
      >{session.running ? 'Cancel and close' : 'Close'}</button
    >
    <button disabled={!session.canAccept} onclick={() => void accept()}
      >Accept timings</button
    >
  </div>
</dialog>

<style>
  dialog {
    color: #e7eef3;
    background: #1e303d;
    border: 1px solid #638299;
    border-radius: 8px;
    padding: 20px;
    width: min(1080px, calc(100vw - 24px));
    max-height: calc(100dvh - 24px);
    overflow: auto;
  }
  dialog::backdrop {
    background: #0009;
  }
  h2 {
    margin-top: 0;
  }
  p,
  dd {
    overflow-wrap: anywhere;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 8px 16px;
  }
  dt {
    font-weight: bold;
  }
  dd {
    margin: 0;
  }
  dd button {
    margin-left: 8px;
  }
  button {
    color: inherit;
    background: #2d4354;
    border: 1px solid #638299;
    border-radius: 4px;
    padding: 6px 10px;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .actions,
  nav {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 16px;
  }
  .start {
    justify-content: flex-start;
  }
  .error,
  .unresolved {
    color: #ffc4b7;
  }
  .draft {
    border: 1px solid #d2b068;
    padding: 12px;
  }
  .table-scroll {
    max-height: 28vh;
    overflow: auto;
    border: 1px solid #638299;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    font-size: 0.9rem;
  }
  th,
  td {
    padding: 8px;
    border-bottom: 1px solid #405668;
    text-align: left;
    vertical-align: top;
  }
  th {
    position: sticky;
    top: 0;
    background: #263e4e;
  }
  td {
    white-space: nowrap;
  }
  .cue-text {
    white-space: pre-wrap;
    min-width: 12em;
    max-width: 28em;
    overflow-wrap: anywhere;
  }
  @media (max-width: 600px) {
    dl {
      display: block;
    }
    dd {
      margin: 4px 0 12px;
    }
  }
</style>
