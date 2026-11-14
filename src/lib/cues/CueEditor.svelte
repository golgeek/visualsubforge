<script lang="ts">
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { MediaSession } from '../media/media-session.svelte';
  import type { QualitySession } from '../quality/quality-session.svelte';
  import type { Workspace } from '../workspace/workspace.svelte';
  import CueMetrics from '../quality/CueMetrics.svelte';
  import { formatTime } from '../timeline/timeline';
  import { measurePaneControl } from '../layout/measure-pane-control';
  import type { TextFormat } from '../client';
  import type { AuthoringModal } from '../dialogs/workspace-modal.svelte';
  let {
    subtitles,
    media,
    quality,
    workspace,
    metrics: showQualityMetrics,
    draftMetricError,
    textEditor = $bindable(),
    editorFocused = $bindable(false),
    onmeasure,
    actions,
  }: {
    subtitles: DocumentSession;
    media: MediaSession;
    quality: QualitySession;
    workspace: Workspace;
    metrics: boolean;
    draftMetricError: string;
    textEditor: HTMLTextAreaElement | undefined;
    editorFocused: boolean;
    onmeasure: (part: 'bar' | 'footer' | 'metrics', height: number) => void;
    actions: {
      suspendCueFollowing: () => void;
      openAuthoring: (kind: AuthoringModal) => Promise<void>;
      formatText: (style: TextFormat) => Promise<void>;
    };
  } = $props();
  const status = $derived(media.status);
  const busy = $derived(workspace.busy);
  const confirming = $derived(workspace.confirming);
  const ready = $derived(workspace.ready);
  const doc = $derived(subtitles.snapshot);
  const cue = $derived(subtitles.cue);
  const selected = $derived(subtitles.selected);
  const draftDirty = $derived(subtitles.dirty);
  const timeStepMs = $derived(doc.format?.timeStepMs ?? 1);
  const textInterpretation = $derived(
    doc.format?.name === 'vtt' ? 'webvtt_utf16_v1' : 'vss_utf16_v1',
  );

  function editorKeydown(e: KeyboardEvent) {
    if (
      (e.metaKey || e.ctrlKey) &&
      !e.altKey &&
      !e.shiftKey &&
      !e.repeat &&
      !e.isComposing &&
      ['b', 'i', 'u'].includes(e.key.toLowerCase())
    ) {
      e.preventDefault();
      void actions.formatText(
        e.key.toLowerCase() === 'b'
          ? 'bold'
          : e.key.toLowerCase() === 'i'
            ? 'italic'
            : 'underline',
      );
      return;
    }
    if (
      (e.metaKey || e.ctrlKey) &&
      e.key === 'Enter' &&
      !e.altKey &&
      !e.shiftKey &&
      !e.repeat &&
      !e.isComposing
    ) {
      e.preventDefault();
      void workspace.run('apply', e);
    }
  }
</script>

<section
  class="editor panel"
  aria-label="Selected subtitle editor"
  onfocusin={(e) => {
    editorFocused =
      e.target instanceof HTMLInputElement ||
      e.target instanceof HTMLSelectElement ||
      e.target instanceof HTMLTextAreaElement;
    if (editorFocused) actions.suspendCueFollowing();
  }}
  onfocusout={() => (editorFocused = false)}
>
  <div
    class="editorbar"
    use:measurePaneControl={(height) => onmeasure('bar', height)}
  >
    <strong
      >{cue
        ? `Cue ${doc.cues.findIndex((c) => c.id === selected) + 1}`
        : 'Cue editor'}</strong
    ><label
      >Start <input
        aria-label="Cue start"
        bind:value={subtitles.start}
        disabled={!cue}
        readonly={busy}
        spellcheck="false"
      /></label
    ><label
      >End <input
        aria-label="Cue end"
        bind:value={subtitles.end}
        disabled={!cue}
        readonly={busy}
        spellcheck="false"
      /></label
    ><button
      disabled={!cue ||
        busy ||
        !status.loaded ||
        status.positionSeconds === null}
      onclick={() => {
        if (status.positionSeconds !== null)
          subtitles.start = formatTime(
            Math.round((status.positionSeconds * 1000) / timeStepMs) *
              timeStepMs,
          );
      }}>Start here</button
    ><button
      disabled={!cue ||
        busy ||
        !status.loaded ||
        status.positionSeconds === null}
      onclick={() => {
        if (status.positionSeconds !== null)
          subtitles.end = formatTime(
            Math.round((status.positionSeconds * 1000) / timeStepMs) *
              timeStepMs,
          );
      }}>End here</button
    >
    {#if doc.format && subtitles.styled}
      <label
        >Style <select
          aria-label="Cue style"
          bind:value={subtitles.style}
          disabled={!cue || busy || confirming}
        >
          {#if cue && !doc.format.styles.some((style) => style.name === subtitles.style)}<option
              value={subtitles.style}>{subtitles.style} (undefined)</option
            >{/if}
          {#each doc.format.styles as style (style.name)}<option
              value={style.name}>{style.name}</option
            >{/each}
        </select></label
      >
      <button
        disabled={busy || confirming || !ready}
        onclick={() => void actions.openAuthoring('styles')}
        >Inspect styles…</button
      >
      <button
        type="button"
        disabled={busy || confirming || !ready}
        onclick={() => void actions.openAuthoring('fonts')}
        >Manage fonts…</button
      >
      <small
        >{doc.format.name.toUpperCase()} · {timeStepMs} ms · line breaks save as \N</small
      >
    {:else if doc.format?.name === 'vtt'}
      <small
        title="WebVTT settings, style/region records and identifiers are preserved. Native preview may omit CSS and region layout or show ruby annotations inline. Timed text may differ from browsers."
        >WebVTT · 1 ms · Preview may differ from browsers</small
      >
    {/if}
    <button
      type="button"
      disabled={!doc.loaded ||
        doc.format?.name === 'vtt' ||
        busy ||
        confirming ||
        !ready}
      title={doc.format?.name === 'vtt'
        ? 'WebVTT conversion is not yet available; use Save As to preserve the source format.'
        : undefined}
      onclick={() => void workspace.run('convertSubtitles')}
      >Convert subtitles…</button
    >
  </div>
  {#if showQualityMetrics && cue}<div
      class="quality-readout"
      use:measurePaneControl={(height) => onmeasure('metrics', height)}
    >
      {#if quality.view?.selected?.data.cueId === String(cue.id)}<CueMetrics
          value={quality.view.selected}
          label="Accepted"
        />{:else}<span
          >Accepted metrics {quality.paused ? 'paused' : 'pending'}…</span
        >{/if}
      {#if draftDirty}
        {#if draftMetricError || quality.view?.draftError}<span role="status"
            >Draft metrics unavailable: {draftMetricError ||
              quality.view?.draftError}</span
          >
        {:else if quality.view?.draft?.data.cueId === String(cue.id) && !quality.stale}<CueMetrics
            value={quality.view.draft}
            label="Draft · not applied"
          />{:else}<span
            >Draft metrics {quality.paused ? 'paused' : 'pending'}…</span
          >{/if}
      {/if}
      <small
        >{doc.format?.name === 'vtt' ? 'Caption text' : 'Source text'}: {textInterpretation}
        · {quality.view?.profile.speedBasis ?? 'loading profile'}. {doc.format
          ?.name === 'vtt'
          ? 'Entities are decoded; voice labels and ruby annotations are excluded.'
          : 'Counting excludes complete tags;'} Rendered wrapping is not measured.</small
      >
      {#if quality.error || quality.view?.error}<span role="alert"
          >{quality.error || quality.view?.error}</span
        >{/if}
    </div>{/if}
  <textarea
    aria-label="Subtitle text"
    bind:this={textEditor}
    bind:value={subtitles.text}
    disabled={!cue && !draftDirty}
    readonly={busy || !cue}
    placeholder="Select a subtitle to edit its text."
    spellcheck="false"
    onkeydown={editorKeydown}></textarea>
  <div
    class="editorfooter"
    use:measurePaneControl={(height) => onmeasure('footer', height)}
  >
    <button
      disabled={!cue || busy || confirming}
      title="Bold selection (Ctrl/Cmd+B)"
      onmousedown={(e) => e.preventDefault()}
      onclick={() => void actions.formatText('bold')}>Bold</button
    >
    <button
      disabled={!cue || busy || confirming}
      title="Italic selection (Ctrl/Cmd+I)"
      onmousedown={(e) => e.preventDefault()}
      onclick={() => void actions.formatText('italic')}>Italic</button
    >
    <button
      disabled={!cue || busy || confirming}
      title="Underline selection (Ctrl/Cmd+U)"
      onmousedown={(e) => e.preventDefault()}
      onclick={() => void actions.formatText('underline')}>Underline</button
    >
    <button
      disabled={!cue || !draftDirty || busy}
      onclick={(e) => void workspace.run('apply', e)}>Apply edit</button
    ><button disabled={!draftDirty || busy} onclick={() => subtitles.revert()}
      >Revert draft</button
    ><button
      disabled={!doc.canUndo || draftDirty || busy}
      onclick={(e) => void workspace.run('undo', e)}>Undo edit</button
    ><button
      disabled={!doc.canRedo || draftDirty || busy}
      onclick={(e) => void workspace.run('redo', e)}>Redo edit</button
    ><span class="spacer"></span><span
      >{draftDirty
        ? 'Draft · apply to record in history'
        : doc.dirty
          ? 'Unsaved changes'
          : 'Saved state'}</span
    >
  </div>
</section>

<style>
  .panel {
    background: var(--surface-panel);
    position: relative;
    z-index: 2;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .spacer {
    flex: 1;
  }

  .quality-readout {
    display: flex;
    flex-direction: column;
    gap: 3px;
    max-height: 100px;
    flex-shrink: 0;
    overflow: auto;
    font-size: 11px;
  }

  .quality-readout small {
    color: var(--text-muted);
    font-size: 10px;
  }

  .quality-readout [role='alert'] {
    color: var(--err);
  }

  .editor {
    display: flex;
    flex-direction: column;
    padding: 6px 8px;
    gap: 6px;
  }

  .editorbar,
  .editorfooter {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }

  .editorbar label {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .editorbar input {
    width: 116px;
    font-variant-numeric: tabular-nums;
  }

  .editorbar select {
    max-width: 150px;
  }

  .editorbar small {
    color: var(--text-muted);
  }

  .editor textarea {
    resize: none;
    flex: 1;
    min-height: 34px;
    width: 100%;
    font-size: 16px;
    line-height: 1.4;
    padding: 8px;
  }

  .editorfooter span {
    font-size: 11px;
    color: var(--text-muted);
  }

  /* A scaled desktop can be smaller than the normal three-pane workspace.
     Keep every control reachable by scrolling instead of clipping fixed rows.
     This changes presentation only; drafts and accepted state stay mounted. */
  @media (max-width: 799px), (max-height: 609px) {
    .editor {
      min-height: 210px;
    }
  }
</style>
