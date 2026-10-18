<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { DocumentSession } from './document-session.svelte';
  import { ConversionSession } from './conversion-session.svelte';
  import type { ConversionOptions } from './client';
  let {
    subtitles,
    onclose,
  }: { subtitles: DocumentSession; onclose: (outputPath?: string) => void } =
    $props();
  const session = untrack(() => new ConversionSession(subtitles));
  const formats: ConversionOptions['target'][] = ['srt', 'ass', 'ssa'];
  const report = $derived(session.view?.report);
  const blocked = $derived(
    report?.issues.some((issue) => issue.severity === 'blocked'),
  );
  let dialog: HTMLDialogElement;
  onMount(() => {
    dialog.showModal();
    const timer = setInterval(() => void session.poll(), 400);
    return () => {
      clearInterval(timer);
      session.dispose();
    };
  });
  function close() {
    if (session.canClose) onclose(session.view?.outputPath ?? undefined);
  }
  function targetChanged(value: string) {
    const target = formats.find((format) => format === value);
    if (target) session.configure({ ...session.options, target });
  }
  function precisionChanged(value: string) {
    if (value === 'exact' || value === 'nearest')
      session.configure({ ...session.options, precision: value });
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="conversion-dialog-title"
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
>
  <header>
    <h2 id="conversion-dialog-title">Convert subtitles</h2>
    <button disabled={!session.canClose} onclick={close}
      >Close conversion</button
    >
  </header>
  <p>
    Review changes and losses before exporting a new file. Your open subtitles,
    saved state and Undo history stay unchanged.
  </p>
  {#if subtitles.dirty}
    <section aria-label="Unapplied cue draft">
      <p>Apply or discard the current cue draft before preparing conversion.</p>
      <button
        disabled={!session.canConfigure}
        onclick={() => void session.applyDraft()}>Apply cue draft</button
      >
      <button
        disabled={!session.canConfigure}
        onclick={() => subtitles.revert()}>Discard cue draft</button
      >
    </section>
  {/if}
  <fieldset disabled={!session.canConfigure}>
    <legend>Conversion options</legend>
    <p>Source: {session.source.toUpperCase()}</p>
    <label
      >Output format <select
        aria-label="Conversion format"
        value={session.options.target}
        onchange={(event) => targetChanged(event.currentTarget.value)}
      >
        {#each formats.filter((format) => format !== session.source) as format (format)}<option
            value={format}>{format.toUpperCase()}</option
          >{/each}
      </select></label
    >
    <label
      >Timing precision <select
        aria-label="Conversion precision"
        value={session.options.precision}
        disabled={session.options.target === 'srt'}
        onchange={(event) => precisionChanged(event.currentTarget.value)}
      >
        <option value="exact">Exact — refuse unrepresentable times</option>
        <option value="nearest">Round to nearest 10 ms — review changes</option>
      </select></label
    >
    <p class="note">
      ASS and SSA require 10 ms endpoints. Rounding can change durations, gaps
      and overlaps; unsafe timed effects block export.
    </p>
  </fieldset>
  <div class="actions">
    <button disabled={!session.canStart} onclick={() => void session.start()}
      >Review conversion</button
    >
    <button
      disabled={!session.running || session.pending}
      onclick={() => void session.cancel()}
      >{session.publishing ? 'Cancel export' : 'Cancel conversion'}</button
    >
  </div>
  {#if session.pending && !session.picking && !session.publishing}<p
      role="status"
    >
      Preparing conversion review…
    </p>{/if}
  {#if session.picking}<p role="status">Choose a new output filename…</p>{/if}
  {#if session.publishing}<p role="status">
      Exporting reviewed subtitles…
    </p>{/if}
  {#if session.error}<p role="alert">{session.error}</p>{/if}
  {#if session.view?.phase === 'preparing' && !session.pending}<p role="status">
      Preparing conversion review…
    </p>{/if}
  {#if session.view?.phase === 'cancelling'}<p role="status">
      Cancelling conversion…
    </p>{/if}
  {#if session.view?.phase === 'cancelled'}<p role="status">
      Conversion cancelled. Prepare a new review to continue.
    </p>{/if}
  {#if session.view?.phase === 'stale'}<p role="alert">
      This review is out of date. Review conversion again.
    </p>{/if}
  {#if session.view?.phase === 'published'}
    <section aria-label="Conversion exported">
      <h3>Export complete</h3>
      <p class="path">{session.view.outputPath}</p>
      <p>
        The original document remains open. Use Open Subtitles to edit the
        exported file.
      </p>
    </section>
  {/if}
  {#if report && (session.current || session.view?.phase === 'published')}
    <section aria-label="Conversion loss report">
      <h3>
        {report.source.toUpperCase()} → {report.options.target.toUpperCase()}
      </h3>
      <p>
        {report.cues} cues. {#if report.outputBytes !== null}Output: {report.outputBytes.toLocaleString()}
          bytes.{/if}
      </p>
      {#if blocked}<p role="alert">
          Export is blocked. Resolve the findings below, then prepare another
          review.
        </p>{/if}
      <p class="note">
        Counts cover the whole document. Each finding shows at most eight
        examples; long examples are shortened.
      </p>
      {#each report.issues as issue (issue.code)}
        <article
          class:blocked={issue.severity === 'blocked'}
          class:loss={issue.severity === 'loss'}
        >
          <h4>
            {issue.severity === 'blocked'
              ? 'Blocking finding'
              : issue.severity === 'loss'
                ? 'Loss or compatibility change'
                : 'Conversion detail'} · {issue.count}
          </h4>
          <p>{issue.message}</p>
          {#if issue.examples.length}<details>
              <summary>Show {issue.examples.length} examples</summary>
              <ul>
                {#each issue.examples as example, index (index)}<li>
                    {#if example.cueId !== null}<strong
                        >Cue {example.cueId}:</strong
                      >
                    {/if}{example.detail}
                  </li>{/each}
              </ul>
            </details>{/if}
        </article>
      {/each}
      {#if session.view?.phase !== 'published'}
        <p>Choose a new filename. Existing files are protected.</p>
        <button
          disabled={!session.canPublish}
          onclick={() => void session.publish()}>Choose new output file…</button
        >
      {/if}
    </section>
  {/if}
</dialog>

<style>
  dialog {
    color: #dceaf1;
    background: #172b37;
    border: 1px solid #5e8196;
    border-radius: 8px;
    padding: 18px;
    width: min(780px, 90vw);
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
    margin-top: 12px;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  h3 {
    font-size: 16px;
  }
  h4 {
    margin: 0;
    font-size: 14px;
  }
  fieldset {
    border: 1px solid #466475;
  }
  label {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    margin: 8px 0;
  }
  select {
    min-width: 0;
    max-width: 100%;
  }
  article {
    border-left: 3px solid #68889b;
    padding: 10px 12px;
    margin: 12px 0;
    background: #1c3442;
  }
  article.blocked {
    border-color: #f49898;
  }
  article.loss {
    border-color: #e6c279;
  }
  p,
  li,
  .path {
    overflow-wrap: anywhere;
  }
  summary {
    cursor: pointer;
  }
  li {
    margin: 6px 0;
    white-space: pre-wrap;
  }
  .note {
    color: #b4cad6;
    font-size: 12px;
  }
</style>
