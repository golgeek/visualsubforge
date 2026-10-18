<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { DocumentSession } from './document-session.svelte';
  import { TextSession } from './text-session.svelte';
  import type { TextOperation, TextTarget, TextView } from './client';
  let {
    subtitles,
    initial,
    onclose,
    onaccepted,
    onnavigate,
  }: {
    subtitles: DocumentSession;
    initial: TextOperation;
    onclose: (operation: TextOperation) => void;
    onaccepted: (operation: TextOperation) => void;
    onnavigate: (
      cue: number,
      start: number,
      end: number,
      operation: TextOperation,
    ) => void;
  } = $props();
  const session = untrack(() => {
    const s = new TextSession(subtitles);
    s.mode = initial.kind;
    if (initial.kind === 'find' || initial.kind === 'replace') {
      s.query = initial.query;
      s.caseSensitive = initial.caseSensitive;
      if (initial.kind === 'replace') s.replacement = initial.replacement;
    }
    if (
      initial.kind === 'languageSuggestions' ||
      initial.kind === 'languageCorrection'
    ) {
      s.languageTarget = {
        cueIndex: initial.cueIndex,
        start: initial.start,
        end: initial.end,
        original: initial.original,
      };
      if (initial.kind === 'languageCorrection')
        s.replacement = initial.replacement;
    }
    return s;
  });
  const view = $derived(session.view);
  const searchable = $derived(
    session.mode === 'find' || session.mode === 'replace',
  );
  const language = $derived(
    session.mode === 'languageSuggestions' ||
      session.mode === 'languageCorrection',
  );
  let dialog: HTMLDialogElement;
  let focus: HTMLSelectElement;
  let matches = $state.raw<{
    cue: number;
    offset: number;
    rows: TextView['rows'][number]['matches'];
  } | null>(null);
  let matchPending = $state(false);
  let matchEpoch = 0;
  onMount(() => {
    dialog.showModal();
    focus.focus();
    const timer = setInterval(() => void session.poll(), 400);
    return () => {
      clearInterval(timer);
      ++matchEpoch;
      session.dispose();
    };
  });
  async function start() {
    ++matchEpoch;
    matches = null;
    await session.start();
  }
  async function accept(target: TextTarget = null) {
    if (await session.accept(target)) onaccepted(session.operation);
  }
  async function matchPage(cue: number, offset: number) {
    const epoch = ++matchEpoch;
    matchPending = true;
    try {
      const rows = await session.matches(cue, offset);
      if (epoch === matchEpoch) matches = { cue, offset, rows };
    } catch (e) {
      if (epoch === matchEpoch) session.error = String(e);
    } finally {
      if (epoch === matchEpoch) matchPending = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="text-title"
  oncancel={(e) => {
    e.preventDefault();
    if (session.canClose) onclose(session.operation);
  }}
>
  <h2 id="text-title">
    {language ? 'Review language correction' : 'Find, replace and repair text'}
  </h2>
  <p>Review accepted subtitles. Every accepted batch creates one Undo step.</p>
  {#if subtitles.dirty}
    <section aria-label="Unapplied cue draft" class="draft">
      <p>
        Apply or discard the current cue draft before searching or previewing
        repairs.
      </p>
      <button
        disabled={!session.canStart}
        onclick={() => void session.applyDraft()}>Apply cue draft</button
      >
      <button disabled={!session.canStart} onclick={() => subtitles.revert()}
        >Discard cue draft</button
      >
      <button
        disabled={!session.canClose}
        onclick={() => onclose(session.operation)}>Cancel text tools</button
      >
    </section>
  {/if}
  <div class="inputs">
    <label
      >Operation <select
        bind:this={focus}
        bind:value={session.mode}
        disabled={session.pending || session.accepting || language}
      >
        <option value="find">Find</option><option value="replace"
          >Find and replace</option
        >
        <option value="joinLines">Join lines</option><option value="reflow"
          >Reflow lines</option
        >
        {#if language}<option value="languageSuggestions"
            >Spelling suggestions</option
          ><option value="languageCorrection">Language correction</option>{/if}
      </select></label
    >
    {#if searchable}
      <label
        >Find text <input
          aria-label="Find text"
          bind:value={session.query}
          disabled={session.pending || session.accepting}
        /></label
      >
      <label
        ><input
          type="checkbox"
          bind:checked={session.caseSensitive}
          disabled={session.pending || session.accepting}
        /> Match case</label
      >
    {/if}
    {#if language}<p>
        Cue {session.languageTarget.cueIndex + 1}: “{session.languageTarget
          .original}”
      </p>{/if}
    {#if session.mode === 'replace' || session.mode === 'languageCorrection'}
      <label
        >Replace with <input
          aria-label="Replace with"
          bind:value={session.replacement}
          disabled={session.pending || session.accepting}
        /></label
      >
    {/if}
    <button
      disabled={!session.canStart ||
        subtitles.dirty ||
        (searchable && !session.query)}
      onclick={() => void start()}
    >
      {session.mode === 'find'
        ? 'Find matches'
        : session.mode === 'languageSuggestions'
          ? 'Get spelling suggestions'
          : 'Preview text changes'}
    </button>
    {#if session.running}<button
        disabled={session.pending || session.accepting}
        onclick={() => void session.cancel()}>Cancel text preview</button
      >{/if}
  </div>
  <details>
    <summary>Search and repair rules</summary>
    <p>
      Literal Unicode search ignores supported tags. Case-insensitive search
      uses Unicode simple folding without normalization. Replacement is plain
      text; matches crossing tags or ASS escapes are skipped. Unknown overrides,
      drawings, karaoke and malformed tags need manual review.
    </p>
    <p>
      Joining and reflow use the active quality profile. Only spaces and
      physical newlines change. Dialogue, positioned text and ASS text escapes
      are skipped. Words and nonbreaking spaces stay intact. Cues above 64 KiB
      are skipped; searches stop above 100,000 matches.
    </p>
  </details>
  <p role="status">
    {#if session.accepting}Applying reviewed text…
    {:else if session.pending}Working…
    {:else if view?.phase === 'preparing'}Preparing text preview…
    {:else if view?.phase === 'cancelling'}Cancelling…
    {:else if view?.phase === 'cancelled'}Text preview cancelled.
    {:else if view?.phase === 'stale'}Subtitles or the quality profile changed.
      Preview again.
    {:else if view?.phase === 'reviewing' && session.current}{view.matches} matches
      · {view.changed} changed cues · {view.total} review rows
    {:else if view?.phase === 'reviewing'}Search inputs changed. Preview again.
    {:else}Search or create a preview to begin.{/if}
  </p>
  {#if session.error}<p role="alert" class="error">{session.error}</p>{/if}
  {#if view?.phase === 'reviewing' && session.current}
    {#if session.mode === 'languageSuggestions'}
      <section aria-label="Spelling suggestions">
        {#if !view.suggestions.length}<p>
            No bounded dictionary suggestions. Edit this word manually or add it
            to your personal words.
          </p>{/if}
        {#each view.suggestions as suggestion (suggestion)}
          <button
            disabled={!session.canStart}
            onclick={() => {
              session.replacement = suggestion;
              session.mode = 'languageCorrection';
              void start();
            }}>Preview “{suggestion}”</button
          >
        {/each}
      </section>
    {/if}
    <p>
      {view.profileName} · Quality findings: {view.beforeFindings} before → {view.afterFindings}
      after (whole preview).
    </p>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard access to independently scrolling results.) -->
    <div class="results" tabindex="0" role="region" aria-label="Text proposal">
      {#each view.rows as row (row.cueId)}
        {@const offset = matches?.cue === row.cueIndex ? matches.offset : 0}
        {@const shown =
          matches?.cue === row.cueIndex ? matches.rows : row.matches}
        <section class="row">
          <h3>Cue {row.cueIndex + 1}</h3>
          {#if row.skipped}<p class="skip">Skipped: {row.skipped}</p>{/if}
          <div class="comparison">
            <div>
              <strong>Before</strong>
              <pre>{row.before}</pre>
            </div>
            {#if row.after !== null}<div>
                <strong>After</strong>
                <pre>{row.after}</pre>
              </div>{/if}
          </div>
          {#if !row.matches.length}<button
              disabled={!session.canClose || !session.current}
              onclick={() => onnavigate(row.cueId, 0, 0, session.operation)}
              >Go to cue {row.cueIndex + 1}</button
            >{/if}
          {#each shown as match, index (index)}
            <div class="match">
              <button
                disabled={!session.canClose || !session.current}
                onclick={() =>
                  onnavigate(
                    row.cueId,
                    match.start,
                    match.end,
                    session.operation,
                  )}
                >Go to match {offset + index + 1} in cue {row.cueIndex +
                  1}</button
              >
              {#if session.mode === 'replace'}<button
                  disabled={!session.canAccept || match.skipped !== null}
                  onclick={() =>
                    void accept({
                      cueIndex: row.cueIndex,
                      matchIndex: offset + index,
                    })}>Replace this match</button
                >{/if}
              {#if match.skipped}<span class="skip">{match.skipped}</span>{/if}
            </div>
          {/each}
          {#if row.matchCount > 100}<nav aria-label="Matches in cue">
              <button
                disabled={matchPending || offset === 0}
                onclick={() =>
                  void matchPage(row.cueIndex, Math.max(0, offset - 100))}
                >Previous matches</button
              >
              <span
                >{offset + 1}–{offset + shown.length} of {row.matchCount}</span
              >
              <button
                disabled={matchPending ||
                  offset + shown.length >= row.matchCount}
                onclick={() => void matchPage(row.cueIndex, offset + 100)}
                >Next matches</button
              >
            </nav>{/if}
        </section>
      {/each}
    </div>
    {#if view.total > 100}<nav aria-label="Text result pages">
        <button
          disabled={!session.canStart || !view.offset}
          onclick={() => void session.page(Math.max(0, view.offset - 100))}
          >Previous cues</button
        >
        <span
          >{view.offset + 1}–{view.offset + view.rows.length} of {view.total}</span
        >
        <button
          disabled={!session.canStart ||
            view.offset + view.rows.length >= view.total}
          onclick={() => void session.page(view.offset + 100)}>Next cues</button
        >
      </nav>{/if}
  {/if}
  <div class="actions">
    <button
      disabled={!session.canClose}
      onclick={() => onclose(session.operation)}>Close text tools</button
    >
    <button disabled={!session.canAccept} onclick={() => void accept()}
      >{session.mode === 'replace'
        ? 'Accept Replace All'
        : 'Accept text repairs'}</button
    >
  </div>
</dialog>

<style>
  dialog {
    color: #e7eef3;
    background: #1e303d;
    border: 1px solid #6c8b9d;
    border-radius: 8px;
    width: min(1000px, 92vw);
    max-height: 90dvh;
    overflow: auto;
    padding: 20px;
  }
  dialog::backdrop {
    background: #0009;
  }
  h2 {
    margin-top: 0;
  }
  h3 {
    font-size: 14px;
    margin-block: 0 8px;
  }
  p,
  details {
    font-size: 13px;
    line-height: 1.5;
  }
  .inputs,
  nav,
  .actions,
  .match {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
  label {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .inputs {
    margin-block: 15px;
  }
  input:not([type='checkbox']) {
    width: 210px;
    max-width: 100%;
  }
  .actions {
    justify-content: flex-end;
    margin-top: 16px;
  }
  .results {
    max-height: 44vh;
    overflow: auto;
  }
  .row {
    padding: 12px 4px;
    border-bottom: 1px solid #6c8b9d;
  }
  .comparison {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(260px, 100%), 1fr));
    gap: 12px;
  }
  .comparison > div {
    min-width: 0;
  }
  pre {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-family: inherit;
    font-size: 13px;
    margin-top: 5px;
  }
  .match {
    margin-block: 6px;
  }
  .skip {
    color: #f1d7a2;
    font-size: 12px;
  }
  .draft {
    padding: 10px;
    background: #344d5d;
    margin-block: 10px;
  }
  .error {
    color: #ffb3ad;
  }
</style>
