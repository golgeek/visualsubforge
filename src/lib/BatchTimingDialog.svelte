<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    batchTimingStart,
    batchTimingView,
    batchTimingCancel,
    type BatchTimingRequest,
    type BatchTimingView,
    type MediaStatus,
  } from './client';
  import type { DocumentSession } from './document-session.svelte';
  import { TimingRepairSession } from './timing-repair-session.svelte';
  import { formatTime } from './timeline';
  import { sameBatchRequest } from './batch-timing';
  let {
    subtitles,
    media,
    onclose,
    onaccepted,
  }: {
    subtitles: DocumentSession;
    media: () => MediaStatus;
    onclose: () => void;
    onaccepted: () => void;
  } = $props();
  let scope = $state<'selected' | 'all' | 'following' | 'playhead'>('selected');
  let operation = $state<'shift' | 'align'>('shift');
  let offset = $state('100');
  // Capture, do not chase a moving playback clock while reviewing.
  function currentPlayhead() {
    const s = media();
    return s.loaded && s.paused && !s.seeking && s.positionSeconds !== null
      ? Math.round(s.positionSeconds * 1000)
      : null;
  }
  let playhead = $state<number | null>(untrack(currentPlayhead));
  const timeStep = $derived(subtitles.snapshot.format?.timeStepMs ?? 1);
  const amount = $derived(Number(offset));
  const validOffset = $derived(
    /^[+-]?\d+$/.test(offset.trim()) &&
      Number.isSafeInteger(amount) &&
      amount !== 0 &&
      Math.abs(amount) <= 35_999_999_999 &&
      amount % timeStep === 0,
  );
  const request = $derived.by<BatchTimingRequest | null>(() => {
    const ids = [...subtitles.selectedIds];
    if (operation === 'align')
      return playhead !== null && ids.length
        ? {
            scope: { kind: 'selected', cueIds: ids },
            change: { kind: 'align', playheadMs: playhead },
          }
        : null;
    if (!validOffset) return null;
    const change = {
      kind: 'shift' as const,
      earlier: amount < 0,
      amountMs: Math.abs(amount),
    };
    if (scope === 'all') return { scope: { kind: 'all' }, change };
    if (scope === 'following')
      return subtitles.selected !== null
        ? { scope: { kind: 'following', cueId: subtitles.selected }, change }
        : null;
    if (scope === 'playhead')
      return playhead !== null
        ? { scope: { kind: 'atOrAfter', positionMs: playhead }, change }
        : null;
    return ids.length
      ? { scope: { kind: 'selected', cueIds: ids }, change }
      : null;
  });
  const session = untrack(
    () =>
      new TimingRepairSession<BatchTimingView, BatchTimingRequest | null>(
        subtitles,
        () => media().session,
        {
          start: (mediaSession, doc, input) =>
            input
              ? batchTimingStart(mediaSession, doc, input)
              : Promise.reject(
                  new Error('Choose a valid timing scope and offset.'),
                ),
          view: batchTimingView,
          cancel: batchTimingCancel,
          accept: (id, doc) => subtitles.acceptBatchTiming(id, doc),
        },
        () => request,
        [''],
        sameBatchRequest,
      ),
  );
  const view = $derived(session.view);
  let dialog: HTMLDialogElement;
  let preview: HTMLButtonElement;
  onMount(() => {
    dialog.showModal();
    preview.focus();
    const timer = setInterval(() => void session.poll(), 300);
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
  aria-labelledby="batch-title"
  oncancel={(e) => {
    e.preventDefault();
    if (session.canClose) onclose();
  }}
>
  <h2 id="batch-title">Shift or align cues</h2>
  <p>
    Preview a uniform timing change. Text, tags, styles, durations and cue order
    stay intact. Accepting all rows creates one Undo step.
  </p>
  {#if subtitles.dirty}
    <section aria-label="Unapplied cue draft">
      <p>Apply or discard the cue draft before creating a proposal.</p>
      <button
        disabled={!session.canStart}
        onclick={() => void session.applyDraft()}>Apply cue draft</button
      >
      <button disabled={!session.canStart} onclick={() => subtitles.revert()}
        >Discard cue draft</button
      >
    </section>
  {/if}
  <div class="controls">
    <label
      >Operation <select
        aria-label="Batch operation"
        bind:value={operation}
        disabled={!session.canStart}
        ><option value="shift">Shift timings</option><option value="align"
          >Align selected cues to playhead</option
        ></select
      ></label
    >
    {#if operation === 'shift'}
      <label
        >Scope <select
          aria-label="Batch scope"
          bind:value={scope}
          disabled={!session.canStart}
        >
          <option value="selected"
            >Selected cues ({subtitles.selectedIds.size})</option
          ><option value="all">All cues</option><option value="following"
            >Current cue and following rows</option
          ><option value="playhead">Cues starting at or after playhead</option>
        </select></label
      >
      <label
        >Offset (ms) <input
          aria-label="Batch offset milliseconds"
          bind:value={offset}
          disabled={!session.canStart}
        /></label
      >
      <p>
        Positive moves later; negative moves earlier. Enter an exact multiple of {timeStep}
        ms.
      </p>
    {:else}<p>
        Move the earliest selected start to the captured playhead. All {subtitles
          .selectedIds.size} selected cues move by the same offset.
      </p>{/if}
    {#if operation === 'align' || scope === 'playhead'}
      <p>
        Captured playhead: {playhead === null
          ? 'unavailable — pause media at a settled position first'
          : formatTime(playhead)}. Alignment uses the nearest {timeStep} ms tick;
        ties round later. The exact target is shown below.
      </p>
      <button
        disabled={!session.canStart || currentPlayhead() === null}
        onclick={() => (playhead = currentPlayhead())}
        >Capture paused playhead</button
      >
    {/if}
  </div>
  <p>
    Review overlaps and gaps in Problems after shifting. Cue-relative animation
    and karaoke move with their cues; their original tags remain unchanged.
  </p>
  <div class="actions">
    <button
      bind:this={preview}
      disabled={!session.canStart || !request || subtitles.dirty}
      onclick={() => void session.start()}
      >{view ? 'Preview again' : 'Create batch preview'}</button
    >
    {#if session.running}<button
        disabled={session.pending || session.accepting}
        onclick={() => void session.cancel()}>Cancel proposal</button
      >{/if}
  </div>
  <p role="status">
    {session.pending
      ? 'Working…'
      : view?.phase === 'preparing'
        ? 'Preparing timing changes…'
        : view?.phase === 'cancelled'
          ? 'Proposal cancelled.'
          : view?.phase === 'stale' || (view && !session.current)
            ? 'Workspace, scope or draft changed; preview again.'
            : view?.phase === 'reviewing'
              ? `${view.changed} cues · offset ${view.offsetMs} ms`
              : 'No timings have changed.'}
  </p>
  {#if session.error}<p class="error" role="alert">{session.error}</p>{/if}
  {#if view?.phase === 'reviewing'}
    {#if view.alignedMs !== null}<p>
        Aligned selection start: {formatTime(view.alignedMs)}.
      </p>{/if}
    <div class="table-scroll" role="region" aria-label="Batch timing proposal">
      <table>
        <thead
          ><tr
            ><th>Cue</th><th>Start before → after</th><th>End before → after</th
            ></tr
          ></thead
        ><tbody>
          {#each view.rows as row (row.cueId)}<tr
              ><td>{row.cueIndex + 1}</td><td
                >{formatTime(row.oldStartMs)} → {formatTime(row.newStartMs)}</td
              ><td>{formatTime(row.oldEndMs)} → {formatTime(row.newEndMs)}</td
              ></tr
            >{/each}
        </tbody>
      </table>
    </div>
    {#if view.total > 100}<nav aria-label="Batch proposal pages">
        <button
          disabled={!session.canStart || !view.offset}
          onclick={() => void session.page(Math.max(0, view.offset - 100))}
          >Previous candidates</button
        ><span
          >{view.offset + 1}–{view.offset + view.rows.length} of {view.total}</span
        ><button
          disabled={!session.canStart ||
            view.offset + view.rows.length >= view.total}
          onclick={() => void session.page(view.offset + 100)}
          >Next candidates</button
        >
      </nav>{/if}
  {/if}
  <p>Saving remains separate. Invalid endpoints reject the entire batch.</p>
  <div class="actions end">
    <button disabled={!session.canClose} onclick={onclose}
      >Close batch review</button
    ><button disabled={!session.canAccept} onclick={() => void accept()}
      >Accept batch timings</button
    >
  </div>
</dialog>

<style>
  dialog {
    color: #e7eef3;
    background: #1e303d;
    border: 1px solid #6c8b9d;
    border-radius: 8px;
    width: min(920px, 92vw);
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
  p {
    font-size: 13px;
    line-height: 1.5;
  }
  .controls,
  .actions,
  nav {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .controls p {
    flex-basis: 100%;
    margin-block: 0;
  }
  .controls {
    margin-block: 16px;
  }
  .end {
    justify-content: flex-end;
  }
  .error {
    color: #ffb3ad;
  }
  section {
    padding: 10px;
    background: #344d5d;
  }
  .table-scroll {
    max-height: 38vh;
    overflow: auto;
    margin-block: 12px;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  th,
  td {
    border-bottom: 1px solid #496270;
    text-align: left;
    padding: 8px;
  }
  th {
    position: sticky;
    top: 0;
    background: #263e4d;
  }
</style>
