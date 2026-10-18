<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { DocumentSession } from './document-session.svelte';
  import type { MediaStatus } from './client';
  import { PluginSession } from './plugin-session.svelte';
  import { formatTime } from './timeline';
  let {
    subtitles,
    media,
    context,
    onclose,
    onaccepted,
  }: {
    subtitles: DocumentSession;
    media: () => MediaStatus;
    context: () => string;
    onclose: () => void;
    onaccepted: () => void;
  } = $props();
  function cursor() {
    const s = media();
    return s.loaded && s.paused && !s.seeking && s.positionSeconds !== null
      ? Math.round(s.positionSeconds * 1000)
      : null;
  }
  const session = untrack(
    () =>
      new PluginSession(subtitles, () => ({
        session: media().session,
        quality: context(),
        cursorMs: cursor(),
      })),
  );
  const view = $derived(session.view);
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
  aria-labelledby="plugin-title"
  oncancel={(e) => {
    e.preventDefault();
    if (session.canClose) onclose();
  }}
>
  <header>
    <h2 id="plugin-title">Subtitle plugins</h2>
    <button disabled={!session.canClose} onclick={onclose}>Close plugins</button
    >
  </header>
  <p>
    Run a checker or preview a fix, then accept all reviewed changes as one Undo
    step.
  </p>
  <div class="actions">
    <button disabled={!session.canStart} onclick={() => void session.pick()}
      >Choose plugin…</button
    >
    <span aria-label="Plugin path"
      >{session.path || 'Choose a local manifest.json package.'}</span
    >
  </div>
  {#if subtitles.dirty}<section aria-label="Unapplied cue draft">
      <p>Apply or discard the current cue draft before running a plugin.</p>
      <button
        disabled={!session.canStart}
        onclick={() => void session.applyDraft()}>Apply cue draft</button
      >
      <button disabled={!session.canStart} onclick={() => subtitles.revert()}
        >Discard cue draft</button
      >
    </section>{/if}
  <fieldset disabled={!session.canStart}>
    <legend>Run options</legend>
    <label
      >Scope <select bind:value={session.scope}
        ><option value="all">All cues</option><option value="selected"
          >Selected cues ({subtitles.selectedIds.size})</option
        ></select
      ></label
    >
    <label
      ><input type="checkbox" bind:checked={session.includeCursor} /> Provide paused
      playhead</label
    >
    <p class="note">
      Cursor: {session.includeCursor && cursor() !== null
        ? formatTime(cursor()!)
        : 'unavailable'}. Pause playback before opening this dialog to provide
      its position. Completed scene results and the active quality profile are
      included when available.
    </p>
    <details>
      <summary>Plugin preferences and settings</summary>
      <div class="preferences">
        <label
          >CPS target <input
            aria-label="Plugin CPS target"
            bind:value={session.cps}
            placeholder="Use active profile"
            inputmode="decimal"
          /></label
        >
        <label
          >VSS RS target index <input
            aria-label="Plugin RS target index"
            bind:value={session.rs}
            placeholder="Unavailable"
            inputmode="numeric"
          /></label
        >
        <label
          >Scene filter offset (ms) <input
            aria-label="Plugin scene filter offset"
            bind:value={session.filter}
            inputmode="numeric"
          /></label
        >
      </div>
      <p class="note">
        Leave CPS blank to use the profile. Supply an RS index only if the
        plugin requires it. Scene filter offset is separate from the profile's
        before/after scene offsets.
      </p>
      <div class="actions">
        <button
          onclick={() =>
            session.settings.push({ key: '', type: 'text', value: '' })}
          >Add setting</button
        >
        <button
          disabled={!view || !Object.keys(view.settings).length}
          onclick={() => session.useSettings()}>Edit effective settings</button
        >
      </div>
      <p class="note">
        A first check reports the plugin's effective settings. Overrides must
        use a declared setting name and its original type.
      </p>
      {#each session.settings as setting, index (index)}<div class="setting">
          <label
            >Name <input
              aria-label={`Setting ${index + 1} name`}
              bind:value={setting.key}
              maxlength={128}
            /></label
          >
          <label
            >Type <select
              aria-label={`Setting ${index + 1} type`}
              bind:value={setting.type}
              onchange={(event) => {
                if (event.currentTarget.value === 'boolean')
                  setting.value = 'false';
              }}
              ><option value="text">Text</option><option value="number"
                >Number</option
              ><option value="boolean">Boolean</option></select
            ></label
          >
          <label
            >Value {#if setting.type === 'boolean'}<select
                aria-label={`Setting ${index + 1} value`}
                bind:value={setting.value}
                ><option value="true">True</option><option value="false"
                  >False</option
                ></select
              >{:else}<input
                aria-label={`Setting ${index + 1} value`}
                bind:value={setting.value}
                maxlength={4096}
              />{/if}</label
          >
          <button
            aria-label={`Remove setting ${index + 1}`}
            onclick={() => session.settings.splice(index, 1)}>Remove</button
          >
        </div>{/each}
    </details>
  </fieldset>
  {#if session.validation}<p role="alert">{session.validation}</p>{/if}
  <div class="actions">
    <button
      disabled={!session.canRun}
      onclick={() => void session.start('check')}>Run checker</button
    >
    <button disabled={!session.canRun} onclick={() => void session.start('fix')}
      >Preview plugin fixes</button
    >
    <button
      disabled={!session.running || session.pending || session.accepting}
      onclick={() => void session.cancel()}>Cancel plugin</button
    >
  </div>
  {#if session.pending}<p role="status">Preparing plugin review…</p>{/if}
  {#if session.accepting}<p role="status">Accepting plugin changes…</p>{/if}
  {#if session.error}<p role="alert">{session.error}</p>{/if}
  {#if session.uncertain}<section role="alert">
      <p>
        The acceptance reply could not be confirmed. Refresh accepted subtitles
        before continuing.
      </p>
      <button
        disabled={session.pending || session.accepting}
        onclick={() => void session.reconcile()}
        >Refresh accepted subtitles</button
      >
    </section>{/if}
  {#if view}
    <p role="status">Plugin: {view.phase}</p>
    {#if view.name}<h3>{view.name}</h3>{/if}
    {#if !session.current}<p role="alert">
        Context changed; run the plugin again before accepting.
      </p>{/if}
    {#if view.phase === 'reviewing' && session.current}
      <p>
        {view.scopeCount} cues in scope; {view.selectionCount} selected. Scene coverage:
        {view.sceneCoverage}. Cursor: {view.cursorMs === null
          ? 'unavailable'
          : formatTime(view.cursorMs)}.
      </p>
      <details>
        <summary>Effective preferences and package</summary>
        <dl>
          <dt>Minimum gap</dt>
          <dd>{view.preferences.minimumBlankMs} ms</dd>
          <dt>Duration range</dt>
          <dd>
            {view.preferences.minimumDurationMs}–{view.preferences
              .maximumDurationMs} ms
          </dd>
          <dt>CPS target</dt>
          <dd>{view.preferences.cpsTarget}</dd>
          <dt>RS target index</dt>
          <dd>{view.preferences.rsTargetIndex ?? 'Unavailable'}</dd>
          <dt>Scene filter offset</dt>
          <dd>{view.sceneFilterOffsetMs} ms</dd>
          <dt>Package</dt>
          <dd>{view.packageId}</dd>
          <dt>Fingerprint</dt>
          <dd>{view.fingerprint}</dd>
          {#each Object.entries(view.settings) as [name, value] (name)}<dt>
              {name}
            </dt>
            <dd>{String(value)}</dd>{/each}
        </dl>
      </details>
      <section aria-label="Plugin findings">
        <h3>Findings ({view.findingsTotal})</h3>
        {#if !view.findingsTotal}<p>No findings.</p>{:else}
          <ol start={view.findingsOffset + 1}>
            {#each view.findings as finding, index (index)}<li>
                Cue ID {finding.cueId}: {finding.message}
              </li>{/each}
          </ol>
          <div class="actions">
            <button
              disabled={!session.canStart || view.findingsOffset === 0}
              onclick={() =>
                void session.page(
                  Math.max(0, view.findingsOffset - 50),
                  view.changesOffset,
                )}>Previous findings</button
            >
            <span
              >{view.findingsOffset + 1}–{view.findingsOffset +
                view.findings.length} of {view.findingsTotal}</span
            >
            <button
              disabled={!session.canStart ||
                view.findingsOffset + view.findings.length >=
                  view.findingsTotal}
              onclick={() =>
                void session.page(view.findingsOffset + 50, view.changesOffset)}
              >Next findings</button
            >
          </div>
        {/if}
      </section>
      <section aria-label="Plugin changes">
        <h3>Proposed changes ({view.changesTotal})</h3>
        {#if view.mode === 'check'}<p>
            Checking does not change subtitles. Preview fixes to review any
            proposed changes.
          </p>{:else if !view.changesTotal}<p>No changes to accept.</p>{/if}
        {#each view.rows as row, index (row.cueId)}<article>
            <h4>
              Cue {row.cueIndex + 1} · ID {row.cueId}{row.outsideScope
                ? ' · Outside selected scope'
                : ''}
            </h4>
            {#if row.warning}<p role="note">{row.warning}</p>{/if}
            <div class="comparison">
              {#each [false, true] as after (after)}{@const value = after
                  ? row.after
                  : row.before}
                <div>
                  <strong>{after ? 'After' : 'Before'}</strong>
                  <p>{formatTime(value.startMs)} → {formatTime(value.endMs)}</p>
                  <pre>{value.text.text}</pre>
                  {#if value.text.truncated}<p>
                      Preview truncated ({value.text.totalBytes} bytes total).
                    </p>
                    <button
                      disabled={!session.canStart}
                      onclick={() =>
                        void session.text(view.changesOffset + index, after)}
                      >Read full {after ? 'after' : 'before'} text for cue {row.cueIndex +
                        1}</button
                    >{/if}
                </div>{/each}
            </div>
          </article>{/each}
        {#if view.changesTotal}<div class="actions">
            <button
              disabled={!session.canStart || view.changesOffset === 0}
              onclick={() =>
                void session.page(
                  view.findingsOffset,
                  Math.max(0, view.changesOffset - 50),
                )}>Previous changes</button
            >
            <span
              >{view.changesOffset + 1}–{view.changesOffset + view.rows.length} of
              {view.changesTotal}</span
            >
            <button
              disabled={!session.canStart ||
                view.changesOffset + view.rows.length >= view.changesTotal}
              onclick={() =>
                void session.page(view.findingsOffset, view.changesOffset + 50)}
              >Next changes</button
            >
          </div>{/if}
        {#if session.detail}{@const detail = session.detail}
          <section aria-label="Full plugin text">
            <h4>
              {detail.after ? 'After' : 'Before'} text · change {detail.row + 1}
            </h4>
            <p>
              Bytes {detail.chunk.offset}–{detail.chunk.nextOffset} of {detail
                .chunk.totalBytes}
            </p>
            <pre>{detail.chunk.text}</pre>
            <button
              disabled={!session.canStart || detail.chunk.offset === 0}
              onclick={() => void session.text(detail.row, detail.after)}
              >First text chunk</button
            >
            <button
              disabled={!session.canStart ||
                detail.chunk.nextOffset >= detail.chunk.totalBytes}
              onclick={() =>
                void session.text(
                  detail.row,
                  detail.after,
                  detail.chunk.nextOffset,
                )}>Next text chunk</button
            >
          </section>{/if}
        {#if view.mode === 'fix' && view.changesTotal}<p>
            Accept includes every proposed change across all pages, including
            changes outside the selected scope.
          </p>
          <button disabled={!session.canAccept} onclick={() => void accept()}
            >Accept plugin changes</button
          >{/if}
      </section>
      {#if view.logs.length}<details>
          <summary>Plugin log ({view.logs.length})</summary>
          <pre>{view.logs.join('\n')}</pre>
        </details>{/if}
    {/if}
  {/if}
</dialog>

<style>
  dialog {
    color: #dceaf1;
    background: #172b37;
    border: 1px solid #5e8196;
    border-radius: 8px;
    padding: 18px;
    width: min(900px, 92vw);
    max-height: 85vh;
    overflow: auto;
  }
  dialog::backdrop {
    background: #0009;
  }
  header,
  .actions,
  .setting,
  .preferences {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
  }
  header {
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  h3 {
    font-size: 16px;
  }
  h4 {
    margin: 8px 0;
  }
  fieldset,
  article,
  section {
    margin: 12px 0;
  }
  label {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    margin: 6px 10px 6px 0;
  }
  input:not([type='checkbox']) {
    width: 150px;
  }
  .setting input {
    width: 130px;
  }
  .comparison {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }
  .comparison > div {
    min-width: 0;
  }
  pre {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 300px;
    overflow: auto;
    background: #10222d;
    padding: 8px;
  }
  dl {
    display: grid;
    grid-template-columns: 150px minmax(0, 1fr);
    gap: 6px;
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .note {
    color: #b4cad6;
    font-size: 12px;
  }
  [aria-label='Plugin path'] {
    overflow-wrap: anywhere;
    min-width: 0;
  }
  @media (max-width: 600px) {
    .comparison {
      grid-template-columns: 1fr;
    }
  }
</style>
