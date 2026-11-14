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
  import { GapSession } from './gap-session.svelte';
  import { SceneRepairSession } from './scene-repair-session.svelte';
  import { DurationRepairSession } from './duration-repair-session.svelte';
  import {
    TimingRepairSession,
    type LiveTimingReview,
  } from './timing-repair-session.svelte';
  import { sameBatchRequest } from './batch-timing';
  import { timingGhosts, type TimingPreview } from './review-preview';
  import { formatTime } from '../timeline/timeline';
  import { rate, sceneCoverage } from '../quality/quality-display';
  let {
    kind,
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
    kind: 'gap' | 'duration' | 'scene' | 'batch';
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
    enabled: () => enabled() && (kind !== 'batch' || request !== null),
    selection,
    context: () => context(),
    previewIds: () => previewIds(),
  };
  const session = untrack(() =>
    kind === 'gap'
      ? new GapSession(subtitles, mediaSession, undefined, live)
      : kind === 'scene'
        ? new SceneRepairSession(subtitles, mediaSession, undefined, live)
        : kind === 'duration'
          ? new DurationRepairSession(
              subtitles,
              mediaSession,
              () => null,
              undefined,
              live,
            )
          : new TimingRepairSession<BatchTimingView, BatchTimingRequest | null>(
              subtitles,
              mediaSession,
              {
                start: (media, document, input, subset) =>
                  input
                    ? batchTimingStart(media, document, input, subset)
                    : Promise.reject(new Error('Enter a valid offset.')),
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
    {#if kind === 'batch'}
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
    {#if view && 'coverage' in view && view.coverage !== 'noMedia'}<p
        class="coverage"
      >
        {sceneCoverage(view.coverage)}
      </p>{/if}
    {#if session.error}<p role="alert">{session.error}</p>
    {/if}
    {#if session.error || view?.phase === 'stale' || view?.phase === 'failed'}<button
        onclick={() => session.retry()}>Retry preview</button
      >{/if}
    {#if view?.selectionError}<p role="alert">{view.selectionError}</p>{/if}
    {#if session.current && view?.phase === 'reviewing'}
      {#if 'beforeFindings' in view}<p>
          Findings {view.beforeFindings} → {view.afterFindings}
        </p>{/if}
      {#if 'profile' in view}<details>
          <summary
            >{view.profile.name} · Timing policy{view.profile.limitations.length
              ? ' · Partial coverage'
              : ''}</summary
          >
          <p>
            Minimum gap {view.profile.timing.cueGapMs} ms; shot buffers {view
              .profile.timing.sceneBeforeMs} ms before / {view.profile.timing
              .sceneAfterMs} ms after. The complete selected result must not worsen
            enabled timing constraints.
          </p>
          {#each view.profile.limitations as limitation, index (index)}<p>
              {limitation}
            </p>{/each}
        </details>{/if}
      <div class="changes">
        {#each view.rows as row (row.cueId)}
          {@const beforeStart =
            'oldStartMs' in row ? row.oldStartMs : row.before.data.startMs}
          {@const beforeEnd =
            'oldEndMs' in row ? row.oldEndMs : row.before.data.endMs}
          {@const nextStart = 'newStartMs' in row ? row.newStartMs : null}
          {@const changeable = nextStart !== null || row.newEndMs !== null}
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
              >{#if 'before' in row}<small
                  >CPS {rate(row.before.data.cps)} → {rate(row.after.data.cps)} ·
                  RS
                  {rate(row.before.data.rs)} → {rate(row.after.data.rs)}</small
                >{/if}{#if 'explanation' in row && row.explanation}<small
                  >{row.explanation}</small
                >{/if}</span
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
  </div>
  <footer>
    {#if view && 'coverage' in view && view.coverage === 'noMedia'}<p
        class="coverage"
      >
        No video, so shot changes weren’t checked.
      </p>{/if}
    <button
      class="apply"
      disabled={!session.canAccept || !enabled()}
      onclick={() => void accept()}
      >Apply {view?.changed ?? 0} change{view?.changed === 1 ? '' : 's'}</button
    >
    {#if previewIds().length > 200}<p class="hint">
        Preview shows the first 200 visible cues. Apply includes all selected
        changes across every page.
      </p>{/if}
    <p class="hint">One Undo step · Preview only until applied.</p>
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
  footer p {
    margin: 4px 0;
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
