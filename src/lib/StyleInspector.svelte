<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { DocumentSession } from './document-session.svelte';
  import { StyleSession } from './style-session.svelte';
  let {
    subtitles,
    initial,
    onclose,
    onaccepted,
  }: {
    subtitles: DocumentSession;
    initial: string;
    onclose: () => void;
    onaccepted: () => void;
  } = $props();
  const session = untrack(() => new StyleSession(subtitles));
  const format = $derived(subtitles.snapshot.format);
  let name = $state(
    untrack(() => {
      const styles = subtitles.snapshot.format?.styles ?? [];
      return styles.some((s) => s.name === initial)
        ? initial
        : (styles[0]?.name ?? '');
    }),
  );
  const style = $derived(format?.styles.find((s) => s.name === name));
  const summary = $derived(session.view?.summary);
  let dialog: HTMLDialogElement;
  onMount(() => {
    dialog.showModal();
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
  aria-labelledby="style-inspector-title"
  oncancel={(event) => {
    event.preventDefault();
    if (session.canClose) onclose();
  }}
>
  <header>
    <h2 id="style-inspector-title">
      {format?.name.toUpperCase()} styles and script
    </h2>
    <button disabled={!session.canClose} onclick={onclose}>Close styles</button>
  </header>
  <p>
    Create or edit a style, review the changes, then accept them as one Undo
    step. Assign styles in the cue editor.
  </p>
  {#if subtitles.dirty}
    <section aria-label="Unapplied cue draft">
      <p>Apply or discard the current cue draft before editing styles.</p>
      <button
        disabled={!session.canStart}
        onclick={() => void session.applyDraft()}>Apply cue draft</button
      >
      <button disabled={!session.canStart} onclick={() => subtitles.revert()}
        >Discard cue draft</button
      >
    </section>
  {/if}
  <label
    >Inspect style <select
      aria-label="Inspect style"
      bind:value={name}
      disabled={!!session.mode || session.pending || session.accepting}
    >
      {#each format?.styles ?? [] as item (item.name)}<option value={item.name}
          >{item.name}</option
        >{/each}
    </select></label
  >
  <div class="actions">
    <button
      disabled={!session.canStart || subtitles.dirty || !style}
      onclick={() => void session.begin('edit', name)}>Edit style</button
    >
    <button
      disabled={!session.canStart || subtitles.dirty || !style}
      onclick={() => void session.begin('copy', name)}>Copy style</button
    >
    <button
      disabled={!session.canStart || subtitles.dirty}
      onclick={() => void session.begin('new', name)}>New style</button
    >
  </div>
  {#if session.mode}
    <h3>
      {session.mode === 'edit'
        ? 'Edit'
        : session.mode === 'copy'
          ? 'Copy'
          : 'Create'} style
    </h3>
    <p>
      Inline overrides can change a cue's appearance. Renaming also updates
      unambiguous assignments and named reset tags; review any refusal before
      retrying.
    </p>
    {#if !session.formCurrent}<p role="alert">
        Subtitles changed. Open the style form again before previewing.
      </p>{/if}
    <table aria-label="Edit style fields">
      <thead><tr><th>Field</th><th>Value</th></tr></thead><tbody>
        {#each session.fields as field (field.name)}
          <tr
            ><th scope="row">{field.name}</th><td>
              {#if field.editable}<input
                  aria-label={`Style ${field.name}`}
                  bind:value={field.value}
                  maxlength={field.name.toLowerCase() === 'name' ? 128 : 1024}
                  disabled={session.pending || session.accepting}
                />
              {:else}{field.value}<small>Preserved, read only</small>{/if}
            </td></tr
          >
        {/each}
      </tbody>
    </table>
    <div class="actions">
      <button
        disabled={!session.canStart || !session.formCurrent || subtitles.dirty}
        onclick={() => void session.start()}>Preview style changes</button
      >
      <button
        disabled={!session.running || session.pending || session.accepting}
        onclick={() => void session.cancel()}>Cancel style preview</button
      >
      <button disabled={!session.canStart} onclick={() => session.discardForm()}
        >Discard style draft</button
      >
    </div>
  {:else if style}
    <table aria-label="Style fields">
      <thead><tr><th>Field</th><th>Value</th></tr></thead><tbody>
        {#each style.fields as [field, value] (field)}<tr
            ><th scope="row">{field}</th><td>{value}</td></tr
          >{/each}
      </tbody>
    </table>
  {:else}<p>No styles are defined in this document.</p>{/if}
  {#if session.pending}<p role="status">Preparing style review…</p>{/if}
  {#if session.error}<p role="alert">{session.error}</p>{/if}
  {#if session.view}<p role="status">
      Style preview: {session.view.phase}
    </p>{/if}
  {#if summary && session.view?.phase === 'reviewing' && session.current}
    <section aria-label="Style change review">
      <h3>{summary.created ? 'Create' : 'Update'} {summary.name}</h3>
      <p>
        {summary.affectedCues} cues have known direct references. {summary.changedCues}
        cue references and {summary.changedComments} Comment events change. Inline
        tags and fallback behavior can affect other rendered results.
      </p>
      {#if summary.referenceReviewCues}<p>
          {summary.referenceReviewCues} cues contain ambiguous style resets. They
          remain unchanged; inspect their preview.
        </p>{/if}
      <table aria-label="Proposed style changes">
        <thead><tr><th>Field</th><th>Before</th><th>After</th></tr></thead
        ><tbody>
          {#each summary.fields as field (field.name)}<tr
              ><th scope="row">{field.name}</th><td
                >{field.before ?? '(new)'}</td
              ><td>{field.after}</td></tr
            >{/each}
        </tbody>
      </table>
      {#if !session.view.changed}<p>No style changes to accept.</p>{/if}
      <button disabled={!session.canAccept} onclick={() => void accept()}
        >Accept style changes</button
      >
    </section>
  {/if}
  <details>
    <summary>Script settings</summary>
    <table aria-label="Script settings">
      <tbody>
        {#each format?.scriptInfo ?? [] as [field, value], index (index)}<tr
            ><th scope="row">{field}</th><td>{value}</td></tr
          >{/each}
      </tbody>
    </table>
  </details>
  <p class="note">
    Fonts must be available to the player; embedded-font availability is not
    guaranteed. Unknown fields remain saved.
  </p>
</dialog>

<style>
  dialog {
    color: #dceaf1;
    background: #172b37;
    border: 1px solid #5e8196;
    border-radius: 8px;
    padding: 18px;
    width: min(620px, 90vw);
    max-height: 85vh;
    overflow: auto;
  }
  dialog::backdrop {
    background: #0009;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
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
  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin: 12px 0;
  }
  input {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
  }
  small {
    display: block;
    color: #b4cad6;
  }
  select {
    min-width: 0;
    max-width: 65%;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    margin: 12px 0;
    font-size: 13px;
  }
  th,
  td {
    text-align: left;
    padding: 4px 8px;
    border-bottom: 1px solid #355263;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  th {
    width: 40%;
  }
  .note {
    font-size: 12px;
    color: #b4cad6;
  }
</style>
