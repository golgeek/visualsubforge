<script lang="ts">
  import type { CueEditing } from './cue-editing.svelte';
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { MediaSession } from '../media/media-session.svelte';
  import type { QualitySession } from '../quality/quality-session.svelte';
  import type { Workspace } from '../workspace/workspace.svelte';
  import type { CueFixes } from '../quality/cue-fixes.svelte';
  import CueIssueStrip from '../quality/CueIssueStrip.svelte';
  import TimecodeField from '../ui/TimecodeField.svelte';
  import CueMetrics from '../quality/CueMetrics.svelte';
  import { formatTime } from '../timeline/timeline';
  import { measurePaneControl } from '../layout/measure-pane-control';
  import type { TextFormat, QualityRule } from '../client';
  import type { AuthoringModal } from '../dialogs/workspace-modal.svelte';
  let {
    editing,
    fixes,
    subtitles,
    media,
    quality,
    workspace,
    metrics: showQualityMetrics,
    draftMetricError,
    textEditor = $bindable(),
    editorFocused = $bindable(false),
    onmeasure,
    editorAbove = false,
    onmove,
    actions,
  }: {
    editing: CueEditing;
    fixes: CueFixes;
    subtitles: DocumentSession;
    media: MediaSession;
    quality: QualitySession;
    workspace: Workspace;
    metrics: boolean;
    draftMetricError: string;
    textEditor: HTMLTextAreaElement | undefined;
    editorFocused: boolean;
    editorAbove?: boolean;
    onmove?: () => void;
    onmeasure: (part: 'bar' | 'footer' | 'metrics', height: number) => void;
    actions: {
      showIssues: () => void;
      reviewIssue: (rule: QualityRule) => void;
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

  const lineMetric = $derived(
    !showQualityMetrics || quality.stale
      ? null
      : draftDirty
        ? quality.view?.draft?.data.cueId === String(selected)
          ? quality.view.draft
          : null
        : quality.view?.selected?.data.cueId === String(selected)
          ? quality.view.selected
          : null,
  );

  function editorKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && !e.isComposing) {
      e.preventDefault();
      e.stopPropagation();
      void editing.escape();
      return;
    }
    if (!(e.target instanceof HTMLTextAreaElement)) return;
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
    if (e.target instanceof HTMLTextAreaElement) editing.focusText();
  }}
  onfocusout={(e) => {
    editorFocused = false;
    if (
      e.target instanceof HTMLInputElement ||
      e.target instanceof HTMLTextAreaElement ||
      e.target instanceof HTMLSelectElement
    )
      editing.blur();
  }}
>
  <div
    class="editorbar"
    use:measurePaneControl={(height) => onmeasure('bar', height)}
  >
    {#if onmove}<button
        disabled={!workspace.canAct}
        aria-label={editorAbove
          ? 'Move editor below list'
          : 'Move editor above list'}
        onclick={onmove}>{editorAbove ? '↓' : '↑'}</button
      >{/if}
    <strong
      >{cue
        ? `Cue ${doc.cues.findIndex((c) => c.id === selected) + 1}`
        : 'Cue editor'}</strong
    >
    <TimecodeField
      label="Start"
      bind:value={subtitles.start}
      disabled={!cue}
      readonly={busy || editing.escaping}
      {timeStepMs}
      frameRate={status.loaded ? (status.nominalFrameRate ?? null) : null}
      invalid={!!subtitles.editError}
      onerror={(message) => (subtitles.editError = message)}
      onkeydown={editorKeydown}
    />
    <TimecodeField
      label="End"
      bind:value={subtitles.end}
      disabled={!cue}
      readonly={busy || editing.escaping}
      {timeStepMs}
      frameRate={status.loaded ? (status.nominalFrameRate ?? null) : null}
      invalid={!!subtitles.editError}
      onerror={(message) => (subtitles.editError = message)}
      onkeydown={editorKeydown}
    />
    {#if status.loaded}
      <button
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
    {/if}
    {#if doc.format && subtitles.styled}
      <label
        >Style <select
          onkeydown={editorKeydown}
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
    {#if doc.loaded && doc.format?.name !== 'vtt'}
      <button
        type="button"
        disabled={busy || confirming || !ready}
        onclick={() => void workspace.run('convertSubtitles')}
        >Export As…</button
      >
    {/if}
  </div>
  <div
    class="quality-readout"
    use:measurePaneControl={(height) => onmeasure('metrics', height)}
  >
    <CueIssueStrip
      {quality}
      {fixes}
      cueId={selected}
      disabled={!workspace.canAct}
      onfix={() =>
        void workspace.task(async () => {
          if (await fixes.accept()) workspace.notice = 'Safe cue fixes applied';
        })}
      onmore={actions.showIssues}
      onreview={actions.reviewIssue}
    />
    {#if showQualityMetrics && cue}
      {#if draftDirty}
        {#if draftMetricError || quality.view?.draftError}<span role="status"
            >Draft metrics unavailable: {draftMetricError ||
              quality.view?.draftError}</span
          >
        {:else if quality.view?.draft?.data.cueId === String(cue.id) && !quality.stale}
          <CueMetrics
            value={quality.view.draft}
            profile={quality.view.profile}
            label="Draft"
          />
        {:else}<span
            >Draft metrics {quality.paused ? 'paused' : 'pending'}…</span
          >{/if}
      {:else if quality.view?.selected?.data.cueId === String(cue.id)}
        <CueMetrics
          value={quality.view.selected}
          profile={quality.view.profile}
          label="Accepted"
        />
      {:else}<span
          >Accepted metrics {quality.paused ? 'paused' : 'pending'}…</span
        >{/if}
      {#if quality.error || quality.view?.error}<span role="alert"
          >{quality.error || quality.view?.error}</span
        >{/if}
    {/if}
  </div>
  <div class="text-area">
    <textarea
      onkeydown={editorKeydown}
      aria-label="Subtitle text"
      bind:this={textEditor}
      bind:value={subtitles.text}
      disabled={!cue && !draftDirty}
      readonly={busy || editing.escaping || !cue}
      placeholder="Select a subtitle to edit its text."
      spellcheck="false"
      class:commit-error={!!subtitles.editError}
      aria-invalid={!!subtitles.editError}
      aria-describedby={subtitles.editError ? 'cue-commit-error' : undefined}
      oncompositionstart={() => editing.composition(true)}
      oncompositionend={() => editing.composition(false)}></textarea>
    {#if lineMetric}<aside
        aria-label="Source line character counts"
        title={lineMetric.linesTruncated
          ? 'First 20 source lines; wrapped display lines do not change the source counts'
          : 'Source line character counts; wrapped display lines do not change these counts'}
      >
        {#each lineMetric.data.lineCharacters as count, index (index)}<span
            >L{index + 1}: {count}</span
          >{/each}
        {#if lineMetric.linesTruncated}<span
            >… {lineMetric.lineCount} lines</span
          >{/if}
      </aside>{/if}
  </div>
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
      disabled={(!doc.canUndo && !draftDirty) || busy}
      onclick={(e) => void workspace.run('undo', e)}>Undo edit</button
    ><button
      disabled={!doc.canRedo || draftDirty || busy}
      onclick={(e) => void workspace.run('redo', e)}>Redo edit</button
    ><span class="spacer"></span><span
      >{draftDirty
        ? subtitles.committing
          ? 'Recording…'
          : 'Editing…'
        : doc.dirty
          ? 'Unsaved changes'
          : 'Saved'}</span
    >
    {#if subtitles.editError}<button
        disabled={busy || subtitles.committing}
        onclick={() => void editing.flush().catch(() => {})}>Retry edit</button
      ><span id="cue-commit-error" role="alert">{subtitles.editError}</span
      >{/if}
  </div>
</section>

<style>
  .commit-error {
    outline: 1px solid var(--err);
  }
  #cue-commit-error {
    color: var(--err);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

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

  .quality-readout [role='alert'] {
    color: var(--err);
  }

  .editor {
    display: flex;
    flex-direction: column;
    padding: 3px 8px;
    gap: 3px;
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

  .editorbar select {
    max-width: 150px;
  }

  .editorbar small {
    color: var(--text-muted);
  }

  .text-area {
    display: flex;
    flex: 1;
    min-height: 34px;
    gap: 6px;
  }
  aside {
    display: flex;
    flex-direction: column;
    width: 70px;
    flex-shrink: 0;
    overflow: auto;
    font: 11px/1.6 var(--font-mono);
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
