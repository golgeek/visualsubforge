<script lang="ts">
  import { onMount } from 'svelte';
  import { LegacyImportSession } from './legacy-import-session.svelte';
  let {
    onclose,
    canOpen = true,
  }: {
    onclose: (outputPath?: string, open?: boolean) => void;
    canOpen?: boolean;
  } = $props();
  const session = new LegacyImportSession();
  const report = $derived(session.view?.report);
  let dialog: HTMLDialogElement;
  onMount(() => {
    dialog.showModal();
    const timer = setInterval(() => void session.poll(), 400);
    return () => {
      clearInterval(timer);
      session.dispose();
    };
  });
  function close(open = false) {
    if (open && !canOpen) return;
    if (session.canClose) onclose(session.view?.outputPath ?? undefined, open);
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="legacy-import-title"
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
>
  <header>
    <h2 id="legacy-import-title">Import VisualSubSync project</h2>
    <button disabled={!session.canClose} onclick={() => close()}
      >Close import</button
    >
  </header>
  <p>
    Review a .vssprj file and create a new VSF project. Original files and your
    current workspace stay unchanged.
  </p>
  <fieldset disabled={!session.canConfigure}>
    <legend>Project and file locations</legend>
    <button onclick={() => void session.pick('path')}
      >Choose legacy project…</button
    >
    <p class="path">{session.request.path || 'No project selected'}</p>
    {#each [{ field: 'mediaPath', label: 'Media' }, { field: 'subtitlePath', label: 'Subtitles' }] as input (input.field)}
      <div class="reference">
        <strong>{input.label}</strong>
        <p class="path">
          {session.request[input.field as 'mediaPath' | 'subtitlePath'] ||
            'Use the path stored in the legacy project'}
        </p>
        <button
          disabled={!session.request.path}
          onclick={() =>
            void session.pick(input.field as 'mediaPath' | 'subtitlePath')}
          >Locate {input.label.toLowerCase()}…</button
        >
        <button
          disabled={!session.request[
            input.field as 'mediaPath' | 'subtitlePath'
          ]}
          onclick={() =>
            session.reset(input.field as 'mediaPath' | 'subtitlePath')}
          >Use original {input.label.toLowerCase()} path</button
        >
      </div>
    {/each}
  </fieldset>
  <div class="actions">
    <button disabled={!session.canStart} onclick={() => void session.start()}
      >Review import</button
    >
    <button
      disabled={!session.running || session.pending}
      onclick={() => void session.cancel()}>Cancel import operation</button
    >
  </div>
  {#if session.picking}<p role="status">Choose a file…</p>{/if}
  {#if (session.pending && !session.picking && !session.publishing) || session.view?.phase === 'preparing'}<p
      role="status"
    >
      Inspecting project files…
    </p>{/if}
  {#if session.publishing}<p role="status">Creating reviewed project…</p>{/if}
  {#if session.view?.phase === 'cancelling'}<p role="status">
      Cancelling import operation…
    </p>{/if}
  {#if session.view?.phase === 'cancelled'}<p role="status">
      Import cancelled. Review again to continue.
    </p>{/if}
  {#if session.view?.phase === 'stale'}<p role="alert">
      A source file changed. Review import again before creating a project.
    </p>{/if}
  {#if session.error}<p role="alert">{session.error}</p>{/if}
  {#if session.view?.phase === 'published'}
    <section aria-label="Project created">
      <h3>Project created</h3>
      <p class="path">{session.view.outputPath}</p>
      <p>
        Opening it will ask you to resolve any unsaved subtitles or cue draft.
      </p>
      <button
        disabled={!session.canClose || !canOpen}
        onclick={() => close(true)}>Open created project</button
      >
      {#if !canOpen}<p>
          Media playback is unavailable in this build. The created project can
          be opened in a VSF build with playback support.
        </p>{/if}
    </section>
  {/if}
  {#if report}
    <section aria-label="Legacy import review">
      <h3>Import review</h3>
      <p class="path">{report.sourcePath} · {report.encoding}</p>
      {#each [report.media, report.subtitles] as reference (reference.field)}
        <article>
          <h4>{reference.field}{reference.overridden ? ' · relocated' : ''}</h4>
          <p>
            Original: <span class="path"
              >{reference.original || 'Not specified'}</span
            >
          </p>
          <p>
            Selected: <span class="path"
              >{reference.resolved ||
                reference.candidate ||
                'Choose a local file'}</span
            >
          </p>
          {#if reference.issue}<p role="alert">{reference.issue}</p>{/if}
        </article>
      {/each}
      {#if report.subtitleFormat}<p>
          {report.subtitleFormat.toUpperCase()} · {report.cueCount?.toLocaleString()}
          cues
        </p>{/if}
      <p>
        Paused position: {report.positionMs.toLocaleString()} ms. Waveform start:
        {report.waveformStartMs.toLocaleString()} ms; span: {report.waveformSpanMs.toLocaleString()}
        ms.
      </p>
      <p>
        Quality and timing use your workspace defaults: {report.qualityProfileName}.
      </p>
      <p class="note">
        The project references the selected files. It does not copy or convert
        their contents. Separate legacy audio, peak caches, scripts, reference
        subtitles and layout settings are not imported. VSF can rebuild waveform
        and scene data when you open the project.
      </p>
      {#if report.warnings.length}
        <h4>Settings and compatibility notices ({report.warnings.length})</h4>
        <ul>
          {#each report.warnings as warning, index (index)}
            <li>
              <strong
                >{warning.field}{warning.line === null
                  ? ''
                  : ` (line ${warning.line})`}</strong
              >: {warning.message}
              {#if warning.value !== null}<span class="value"
                  >{warning.value}</span
                >{/if}
            </li>
          {/each}
        </ul>
      {/if}
      {#if session.view?.phase !== 'published'}
        {#if !report.canPublish && session.view?.phase === 'reviewing' && !session.running}<p
            role="alert"
          >
            Resolve the file problems above, then review import again.
          </p>{/if}
        <p>Choose a new .vsfproject filename. Existing files are protected.</p>
        <button
          disabled={!session.canPublish}
          onclick={() => void session.publish()}>Create new VSF project…</button
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
  .reference {
    margin-top: 16px;
  }
  article {
    border-left: 3px solid #68889b;
    padding: 10px 12px;
    margin: 12px 0;
    background: #1c3442;
  }
  p,
  li,
  .path {
    overflow-wrap: anywhere;
  }
  li {
    margin: 10px 0;
  }
  .value {
    display: block;
    white-space: pre-wrap;
    color: #c5d6e1;
  }
  .note {
    color: #b4cad6;
    font-size: 12px;
  }
</style>
