<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    batchTimingStart,
    batchTimingView,
    batchTimingCancel,
    type BatchTimingRequest,
    type BatchTimingView,
    type ReviewSelection,
  } from '../client';
  import type { DocumentSession } from '../document/document-session.svelte';
  import {
    TimingRepairSession,
    type LiveTimingReview,
  } from './timing-repair-session.svelte';
  import { sameBatchRequest } from './batch-timing';
  import { timingGhosts, type TimingPreview } from './review-preview';
  import { formatTime } from '../timeline/timeline';
  import type { WorkspaceActivity } from '../workspace/workspace-activity.svelte';
  let {
    activity,
    subtitles,
    mediaSession,
    playhead,
    onbusy,
    context,
    scopeIds,
    previewIds,
    enabled,
    onpreview,
    onaccepted,
  }: {
    activity: WorkspaceActivity;
    subtitles: DocumentSession;
    mediaSession: () => number;
    playhead: () => number | null;
    onbusy: (busy: boolean) => void;
    context: () => string;
    scopeIds: () => number[] | null;
    previewIds: () => readonly number[];
    enabled: () => boolean;
    onpreview: (value: TimingPreview | null) => void;
    onaccepted: () => void;
  } = $props();
  let excluded = $state<number[]>([]);
  let offset = $state('100');
  let operation = $state<'shift' | 'align'>('shift');
  let anchor = $state<number | null>(untrack(() => playhead()));
  const amount = $derived(Number(offset));
  const valid = $derived(
    /^[+-]?\d+$/.test(offset.trim()) &&
      Number.isSafeInteger(amount) &&
      amount !== 0 &&
      Math.abs(amount) <= 35_999_999_999 &&
      amount % (subtitles.snapshot.format?.timeStepMs ?? 1) === 0,
  );
  const request = $derived.by<BatchTimingRequest | null>(() => {
    const ids = scopeIds();
    const scope: BatchTimingRequest['scope'] =
      ids === null ? { kind: 'all' } : { kind: 'selected', cueIds: ids };
    if (operation === 'align')
      return anchor !== null && (ids === null || ids.length > 0)
        ? { scope, change: { kind: 'align', playheadMs: anchor } }
        : null;
    return valid && (ids === null || ids.length > 0)
      ? {
          scope,
          change: {
            kind: 'shift',
            earlier: amount < 0,
            amountMs: Math.abs(amount),
          },
        }
      : null;
  });
  const baseKey = $derived(
    JSON.stringify([
      subtitles.snapshot.generation,
      subtitles.snapshot.revision,
      scopeIds(),
      offset,
      operation,
      anchor,
      context(),
    ]),
  );
  let exclusionKey = $state('');
  const selection = (): ReviewSelection => ({
    scope: scopeIds(),
    excluded: exclusionKey === baseKey ? excluded : [],
  });
  function toggle(id: number, checked: boolean) {
    const previous = exclusionKey === baseKey ? excluded : [];
    exclusionKey = baseKey;
    excluded = checked
      ? previous.filter((value) => value !== id)
      : [...previous, id];
  }
  const live: LiveTimingReview = {
    enabled: () => enabled() && request !== null,
    activity: untrack(() => activity),
    selection,
    context: () => context(),
    previewIds: () => previewIds(),
  };
  const session = untrack(
    () =>
      new TimingRepairSession<BatchTimingView, BatchTimingRequest | null>(
        subtitles,
        mediaSession,
        {
          start: (media, document, input, subset) =>
            input
              ? batchTimingStart(media, document, input, subset)
              : Promise.reject(new Error('Enter a valid offset.')),
          purpose: 'batch',
          view: batchTimingView,
          cancel: batchTimingCancel,
          accept: (id, doc) => subtitles.acceptBatchTiming(id, doc),
        },
        () => request,
        [''],
        sameBatchRequest,
        live,
      ),
  );
  const view = $derived(session.view);
  $effect(() => {
    const busy = !session.canClose;
    untrack(() => onbusy(busy));
  });
  $effect(() => {
    const preview =
      session.current && session.preview && session.previewDocument
        ? {
            document: session.previewDocument,
            ghosts: timingGhosts(session.preview),
          }
        : null;
    untrack(() => onpreview(preview));
  });
  onMount(() => () => {
    session.dispose();
    onpreview(null);
    onbusy(false);
  });
  async function accept() {
    if (await session.accept()) onaccepted();
  }
  const time = (value: number | null) =>
    value === null ? '—' : formatTime(value);
</script>

<section class="timing-review" aria-label="Live timing review">
  <div class="review-content">
    <label
      >Operation <select
        aria-label="Batch operation"
        bind:value={operation}
        disabled={session.accepting}
        ><option value="shift">Shift</option><option value="align"
          >Align earliest scoped start</option
        ></select
      ></label
    >
    {#if operation === 'align'}<p>
        Captured playhead: {anchor === null
          ? 'Pause video to capture a position.'
          : formatTime(anchor)}
      </p>
      <button
        disabled={session.accepting || playhead() === null}
        onclick={() => (anchor = playhead())}>Capture paused playhead</button
      >
    {:else}
      <label
        >Shift by <input
          aria-label="Shift offset in milliseconds"
          bind:value={offset}
          inputmode="numeric"
          disabled={session.accepting}
        /> ms</label
      >
      <p>
        Positive moves later; negative moves earlier. Text and durations stay
        intact.
      </p>
      {#if !valid}<p role="alert">
          Enter a nonzero whole-millisecond offset divisible by {subtitles
            .snapshot.format?.timeStepMs ?? 1}.
        </p>{/if}
    {/if}
    {#if subtitles.dirty}<p role="status">
        Waiting for the cue edit to commit.
      </p>{/if}
    <p role="status">
      {session.accepting
        ? 'Applying timings…'
        : session.pending || session.running || !session.current
          ? 'Updating preview…'
          : view?.phase === 'reviewing'
            ? `${view.changed} change${view.changed === 1 ? '' : 's'}${'skipped' in view ? ` · ${view.skipped} skipped` : ''}`
            : (view?.phase ?? 'Preparing preview…')}
    </p>
    {#if session.error}<p role="alert">{session.error}</p>
    {/if}
    {#if session.error || view?.phase === 'stale' || view?.phase === 'failed'}<button
        onclick={() => session.retry()}>Retry preview</button
      >{/if}
    {#if view?.selectionError}<p role="alert">{view.selectionError}</p>{/if}
    {#if session.current && view?.phase === 'reviewing'}
      <div class="changes">
        {#each view.rows as row (row.cueId)}
          {@const beforeStart = row.oldStartMs}
          {@const beforeEnd = row.oldEndMs}
          {@const nextStart = row.newStartMs}
          {@const changeable = true}
          <label class="change" class:skipped={!changeable}>
            <input
              type="checkbox"
              aria-label={`Include cue ${row.cueIndex + 1}`}
              checked={row.selected}
              disabled={!changeable || session.accepting}
              onchange={(event) =>
                toggle(Number(row.cueId), event.currentTarget.checked)}
            />
            <span
              ><strong>Cue {row.cueIndex + 1}</strong><small
                >{time(beforeStart)} → {time(nextStart ?? beforeStart)}<br
                />{time(beforeEnd)} → {time(row.newEndMs ?? beforeEnd)}</small
              ></span
            >
          </label>
        {/each}
      </div>
      {#if view.total > 100}<nav aria-label="Timing proposal pages">
          <button
            disabled={!view.offset || session.pending}
            onclick={() => void session.page(Math.max(0, view.offset - 100))}
            >Previous</button
          ><span
            >{view.offset + 1}–{view.offset + view.rows.length} / {view.total}</span
          ><button
            disabled={view.offset + view.rows.length >= view.total ||
              session.pending}
            onclick={() => void session.page(view.offset + 100)}>Next</button
          >
        </nav>{/if}
    {/if}
    {#if previewIds().length > 200}<p class="hint">
        Preview shows the first 200 visible cues. Apply includes all selected
        changes across every page.
      </p>{/if}
    <p class="hint">One Undo step · Preview only until applied.</p>
  </div>
  <footer>
    <button
      class="apply"
      data-variant="primary"
      disabled={!session.canAccept || !enabled()}
      onclick={() => void accept()}
      >Apply {view?.changed ?? 0} change{view?.changed === 1 ? '' : 's'}</button
    >
  </footer>
</section>

<style>
  .timing-review {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 0;
    font-size: 12px;
  }
  .review-content {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 10px;
  }
  footer {
    flex: none;
    padding: 8px 10px;
    border-top: 1px solid var(--line);
  }
  p {
    line-height: 1.4;
    margin: 8px 0;
    color: var(--text-muted);
  }
  [role='alert'] {
    color: var(--err);
  }
  input:not([type='checkbox']) {
    width: 100px;
  }
  .change {
    display: flex;
    gap: 8px;
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
    align-items: start;
  }
  small {
    display: block;
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
    margin-top: 4px;
  }
  .skipped {
    opacity: 0.65;
  }
  nav {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 0;
  }
  .apply {
    width: 100%;
    margin-top: 0;
  }
  .hint {
    font-size: 11px;
  }
</style>
