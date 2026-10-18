<script lang="ts">
  import { onMount } from 'svelte';
  import type { DocumentSession } from './document-session.svelte';
  import type { TranslationSession } from './translation-session.svelte';
  import { formatTime } from './timeline';
  let {
    subtitles,
    session,
    disabled,
    onload,
    onoffset,
    ondetach,
    onclose,
  }: {
    subtitles: DocumentSession;
    session: TranslationSession;
    disabled: boolean;
    onload: (copy: boolean) => void;
    onoffset: (offset: number) => void;
    ondetach: () => void;
    onclose: () => void;
  } = $props();
  const doc = $derived(subtitles.snapshot);
  const reference = $derived(doc.translation?.reference);
  let browse = $state(false),
    offset = $state(0),
    selected = $state<number | null>(null);
  let offsetText = $state('0'),
    offsetError = $state('');
  let identity = '',
    offsetIdentity = '';
  $effect(() => {
    const next = `${doc.generation}/${browse ? '' : doc.revision}/${doc.translation?.epoch ?? 0}/${browse}/${browse ? '' : (subtitles.cue?.id ?? '')}`;
    if (next !== identity) {
      identity = next;
      offset = 0;
      selected = null;
    }
    const sourceOffset = `${reference?.path ?? ''}/${reference?.sha256 ?? ''}/${reference?.offsetMs ?? 0}`;
    if (sourceOffset !== offsetIdentity) {
      offsetIdentity = sourceOffset;
      offsetText = String(reference?.offsetMs ?? 0);
      offsetError = '';
    }
    session.request(
      doc,
      browse ? null : (subtitles.cue?.id ?? null),
      offset,
      selected,
      !disabled && (browse || !!subtitles.cue),
    );
  });
  onMount(
    () => () => session.request(subtitles.snapshot, null, 0, null, false),
  );
  function applyOffset() {
    const value = Number(offsetText);
    if (!/^[+-]?\d+$/.test(offsetText.trim()) || !Number.isSafeInteger(value)) {
      offsetError = 'Enter an exact whole number of milliseconds.';
      return;
    }
    offsetError = '';
    onoffset(value);
  }
  const time = (ms: number) => (ms < 0 ? '−' : '') + formatTime(Math.abs(ms));
</script>

<section class="translation" aria-label="Translation reference">
  <header>
    <strong>Translation reference</strong><button {disabled} onclick={onclose}
      >Close reference panel</button
    >
  </header>
  <div class="actions">
    <button disabled={disabled || !doc.loaded} onclick={() => onload(false)}
      >{reference ? 'Replace reference…' : 'Attach reference…'}</button
    >
    <button {disabled} onclick={() => onload(true)}>Start target copy…</button>
    {#if reference}<button {disabled} onclick={ondetach}
        >Detach reference</button
      >{/if}
    {#if session.loading || session.querying}<button
        onclick={() => session.cancel()}>Cancel reference operation</button
      >{/if}
  </div>
  {#if session.error}<p role="alert">
      {session.error}
      <button {disabled} onclick={() => session.retry()}>Retry reading</button>
    </p>{/if}
  {#if !reference}
    {#if session.loading}<p role="status">Loading reference…</p>{/if}
    <p>
      Attach a read-only SRT, ASS, SSA or WebVTT reference to your target, or
      start an independent copy to translate. The reference file stays
      unchanged.
    </p>
  {:else}
    <p class="source" title={reference.path}>
      {reference.path} · {reference.format.toUpperCase()} · {reference.cues.toLocaleString()}
      cues
    </p>
    <p class="hint">
      Save a project to keep this reference and offset{doc.translation?.dirty
        ? ' · reference changes unsaved'
        : ''}.
    </p>
    <form
      onsubmit={(event) => {
        event.preventDefault();
        applyOffset();
      }}
    >
      <label
        >Reference offset (ms) <input
          aria-label="Reference offset in milliseconds"
          inputmode="numeric"
          bind:value={offsetText}
          {disabled}
        /></label
      >
      <button {disabled}>Apply offset</button>
    </form>
    <p class="hint">
      Reference time + offset = target time. Subtitle timings are unchanged.
    </p>
    {#if offsetError}<p role="alert">{offsetError}</p>{/if}
    <label
      >Reference view <select
        aria-label="Reference view"
        bind:value={browse}
        {disabled}
      >
        <option value={false}>Matching cues</option><option value={true}
          >Browse all</option
        >
      </select></label
    >
    <button {disabled} onclick={() => session.retry()}
      >Refresh reference view</button
    >
    <p class="hint">
      {subtitles.dirty
        ? 'Matching uses accepted timing; the target has an unapplied draft.'
        : 'Matches overlap the selected target cue; touching endpoints do not match.'}
    </p>
    <p class="hint">
      Reading text omits styling and timed effects. Raw text preserves markup.
    </p>
    {#if session.loading || session.querying}<p role="status">
        {session.loading ? 'Loading reference…' : 'Reading reference…'}
      </p>
    {:else if !browse && !subtitles.cue}<p role="status">
        Select a target cue or browse all reference cues.
      </p>
    {:else if session.page}
      <p role="status">
        {session.page.total === 0
          ? 'No matching reference cues.'
          : `${session.page.total.toLocaleString()} ${browse ? 'reference cues' : 'matching cues'}`}
      </p>
      <div class="reference-rows" aria-label="Reference cues">
        {#each session.page.rows as row (row.index)}
          <button
            class:selected={selected === row.index}
            aria-pressed={selected === row.index}
            {disabled}
            onclick={() => (selected = row.index)}
          >
            <strong
              >#{row.index + 1} · {time(row.startMs)} → {time(
                row.endMs,
              )}</strong
            >
            <span
              >{row.reading ?? row.raw}{row.truncated
                ? '… (excerpt)'
                : ''}</span
            >
          </button>
        {/each}
      </div>
      <div class="actions">
        <button
          disabled={disabled || offset === 0}
          onclick={() => {
            offset = Math.max(0, offset - 100);
            selected = null;
          }}>Previous reference page</button
        >
        <button
          disabled={disabled || offset + 100 >= session.page.total}
          onclick={() => {
            offset += 100;
            selected = null;
          }}>Next reference page</button
        >
      </div>
      {#if session.page.selected}{@const row = session.page.selected}
        <section aria-label="Selected reference cue">
          <h3>Reference cue #{row.index + 1}</h3>
          {#if row.warning}<p>{row.warning}</p>{/if}
          {#if row.timed || row.layout}<p class="hint">
              Contains {row.timed ? 'timed effects' : ''}{row.timed &&
              row.layout
                ? ' and '
                : ''}{row.layout ? 'layout markup' : ''}.
            </p>{/if}
          <h4>Reading text</h4>
          <pre>{row.reading ?? 'Reading projection unavailable.'}</pre>
          <h4>Raw text</h4>
          <pre>{row.raw}</pre>
          {#if row.truncated}<p>
              Preview truncated; the complete reference remains preserved.
            </p>{/if}
        </section>
      {/if}
    {/if}
  {/if}
</section>

<style>
  .translation {
    width: min(46%, 620px);
    min-width: 280px;
    padding: 10px;
    overflow: auto;
    border-left: 1px solid #638299;
    background: #203542;
    font-size: 12px;
  }
  header,
  .actions,
  form {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  header {
    justify-content: space-between;
  }
  .source,
  pre {
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }
  .hint {
    color: #b5cbd6;
    font-size: 11px;
  }
  input {
    width: 110px;
  }
  [role='alert'] {
    color: #ffc4b7;
  }
  .reference-rows {
    max-height: 230px;
    overflow: auto;
    margin: 8px 0;
  }
  .reference-rows button {
    display: flex;
    flex-direction: column;
    width: 100%;
    text-align: left;
    gap: 4px;
  }
  .reference-rows span {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .selected {
    outline: 1px solid #80dfc8;
    outline-offset: -2px;
  }
  h3,
  h4 {
    margin-bottom: 4px;
  }
  pre {
    margin-top: 4px;
    font-family: inherit;
  }
</style>
