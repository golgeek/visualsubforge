<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { DocumentSession } from './document-session.svelte';
  import { FontSession } from './font-session.svelte';
  let {
    subtitles,
    onclose,
    onaccepted,
  }: {
    subtitles: DocumentSession;
    onclose: () => void;
    onaccepted: () => void;
  } = $props();
  const session = untrack(() => new FontSession(subtitles));
  const summary = $derived(session.view?.summary);
  let dialog: HTMLDialogElement;
  onMount(() => {
    dialog.showModal();
    void session.inspect();
    const timer = setInterval(() => void session.poll(), 400);
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
  aria-labelledby="font-dialog-title"
  oncancel={(event) => {
    event.preventDefault();
    if (session.canClose) onclose();
  }}
>
  <header>
    <h2 id="font-dialog-title">Font attachments</h2>
    <button disabled={!session.canClose} onclick={onclose}>Close fonts</button>
  </header>
  <p>
    Add, replace or remove a font, review it, then accept as one Undo step.
    Fonts are saved inside the subtitle file.
  </p>
  {#if subtitles.dirty}
    <section aria-label="Unapplied cue draft">
      <p>Apply or discard the current cue draft before changing attachments.</p>
      <button
        disabled={!session.canStart}
        onclick={() => void session.applyDraft()}>Apply cue draft</button
      >
      <button disabled={!session.canStart} onclick={() => subtitles.revert()}
        >Discard cue draft</button
      >
    </section>
  {/if}
  <div class="actions">
    <button disabled={!session.canStart} onclick={() => void session.inspect()}
      >Refresh attachments</button
    >
    <button disabled={!session.canPut} onclick={() => void session.pick()}
      >Add font…</button
    >
    <button
      disabled={!session.canPut || !session.editable}
      onclick={() => void session.pick(true)}>Replace font…</button
    >
    <button
      disabled={!session.canChange || !session.editable}
      onclick={() => void session.remove()}>Preview removal</button
    >
    <button
      disabled={!session.running || session.pending || session.accepting}
      onclick={() => void session.cancel()}>Cancel font review</button
    >
  </div>
  {#if session.pending}<p role="status">Preparing font review…</p>{/if}
  {#if session.error}<p role="alert">{session.error}</p>{/if}
  {#if session.view}<p role="status">Font review: {session.view.phase}</p>{/if}
  {#if session.inventory}
    {#if !session.inventoryCurrent}<p role="alert">
        Subtitles changed. Refresh attachments before editing.
      </p>{/if}
    <p>
      {session.inventory.total} attachment records. Graphics and unsupported fonts
      remain preserved read-only.
    </p>
    {#if !session.inventory.complete}<p role="alert">
        Only the first 200 records are shown. This inventory cannot be edited.
      </p>{/if}
    {#if session.inventory.entries.length}
      <label
        >Attachment <select
          aria-label="Attachment"
          bind:value={session.selected}
          disabled={session.pending || session.accepting || session.running}
        >
          {#each session.inventory.entries as entry (entry.id)}<option
              value={entry.id}>{entry.name} ({entry.kind})</option
            >{/each}
        </select></label
      >
      {#if session.entry}
        <dl aria-label="Attachment details">
          <dt>Filename</dt>
          <dd>{session.entry.name}</dd>
          <dt>Encoded bytes</dt>
          <dd>{session.entry.encodedBytes}</dd>
          <dt>Decoded bytes</dt>
          <dd>{session.entry.decodedBytes ?? 'Unavailable'}</dd>
          <dt>Font family</dt>
          <dd>{session.entry.font?.family ?? 'Unavailable'}</dd>
          <dt>Format</dt>
          <dd>{session.entry.font?.kind ?? 'Unsupported'}</dd>
          <dt>SHA256</dt>
          <dd>{session.entry.sha256 ?? 'Unavailable'}</dd>
        </dl>
        {#if session.entry.problem}<p role="note">
            {session.entry.problem}
          </p>{/if}
      {/if}
    {:else}<p>No font or graphic attachments.</p>{/if}
    {#if session.inventory.entries.some((e) => e.kind === 'font' && e.problem)}<p
      >
        Adding or replacing fonts requires a fully supported font inventory.
      </p>{/if}
  {/if}
  {#if summary && session.current && session.view?.phase === 'reviewing'}
    <section aria-label="Font change review">
      <h3>
        {summary.action === 'add'
          ? 'Add font'
          : summary.action === 'replace'
            ? 'Replace font'
            : 'Remove font'}
      </h3>
      <table aria-label="Proposed font change">
        <thead><tr><th>Property</th><th>Before</th><th>After</th></tr></thead
        ><tbody>
          <tr
            ><th>Filename</th><td>{summary.before?.name ?? '(none)'}</td><td
              >{summary.after?.name ?? '(removed)'}</td
            ></tr
          >
          <tr
            ><th>Family</th><td>{summary.before?.font?.family ?? '—'}</td><td
              >{summary.after?.font?.family ?? '—'}</td
            ></tr
          >
          <tr
            ><th>Decoded bytes</th><td>{summary.before?.decodedBytes ?? 0}</td
            ><td>{summary.after?.decodedBytes ?? 0}</td></tr
          >
          <tr
            ><th>SHA256</th><td>{summary.before?.sha256 ?? '—'}</td><td
              >{summary.after?.sha256 ?? '—'}</td
            ></tr
          >
        </tbody>
      </table>
      <p>
        The resulting subtitle file is {summary.documentBytes} bytes. Cue text, timing
        and font declarations remain unchanged.
      </p>
      {#if summary.action === 'remove'}<p>
          Cues using this font may fall back to an installed font.
        </p>{/if}
      {#if summary.previewSizeExceeded}<p role="alert">
          This file exceeds the 4 MiB native subtitle preview limit. It can be
          saved, but its preview will be unavailable.
        </p>{/if}
      {#if !session.view.changed}<p>No font changes to accept.</p>{/if}
      <button disabled={!session.canAccept} onclick={() => void accept()}
        >Accept font change</button
      >
    </section>
  {/if}
  <p class="note">
    Single TTF/OTF fonts only: 4 MiB each, 8 MiB total, up to 32 fonts. OpenType
    in SSA requires a modern compatible player. Rendering depends on the player;
    structural validation does not guarantee appearance. Attach only fonts you
    are allowed to redistribute with the subtitles.
  </p>
</dialog>

<style>
  dialog {
    color: #dceaf1;
    background: #172b37;
    border: 1px solid #5e8196;
    border-radius: 8px;
    padding: 18px;
    width: min(760px, 90vw);
    max-height: 85vh;
    overflow: auto;
  }
  dialog::backdrop {
    background: #0009;
  }
  header,
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  header {
    justify-content: space-between;
  }
  .actions {
    flex-wrap: wrap;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  label {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  select {
    min-width: 0;
    max-width: 80%;
  }
  dl {
    display: grid;
    grid-template-columns: 130px minmax(0, 1fr);
    gap: 7px;
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  table {
    width: 100%;
    table-layout: fixed;
    border-collapse: collapse;
    margin: 12px 0;
    font-size: 13px;
  }
  th,
  td {
    text-align: left;
    padding: 5px;
    border-bottom: 1px solid #355263;
    overflow-wrap: anywhere;
  }
  .note {
    color: #b4cad6;
    font-size: 12px;
  }
</style>
