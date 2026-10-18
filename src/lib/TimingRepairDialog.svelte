<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { DocumentSession } from './document-session.svelte';
  import { SceneRepairSession } from './scene-repair-session.svelte';
  import { DurationRepairSession } from './duration-repair-session.svelte';
  import { formatTime } from './timeline';
  import { rate, sceneCoverage } from './quality-display';
  let {
    purpose,
    subtitles,
    mediaSession,
    onclose,
    onaccepted,
  }: {
    purpose: 'scene' | 'duration';
    subtitles: DocumentSession;
    mediaSession: () => number;
    onclose: () => void;
    onaccepted: () => void;
  } = $props();
  let scope = $state<'selected' | 'document'>('selected');
  const selectedScope = () =>
    scope === 'document' ? null : (subtitles.selected?.toString() ?? '');
  const session = untrack(() =>
    purpose === 'scene'
      ? new SceneRepairSession(subtitles, mediaSession)
      : new DurationRepairSession(subtitles, mediaSession, selectedScope),
  );
  const id = untrack(() => `${purpose}-repair-title`);
  const isDuration = untrack(() => purpose === 'duration');
  const view = $derived(session.view);
  let dialog: HTMLDialogElement;
  let preview: HTMLButtonElement;
  const time = (value: number | null) =>
    value === null ? 'unavailable' : formatTime(value);
  onMount(() => {
    dialog.showModal();
    preview.focus();
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
  aria-labelledby={id}
  oncancel={(event) => {
    event.preventDefault();
    if (session.canClose) onclose();
  }}
>
  <h2 {id}>
    {isDuration ? 'Repair duration and reading speed' : 'Repair scene timing'}
  </h2>
  <p>
    {#if isDuration}Preview the smallest safe end-time changes that meet the
      active duration and CPS/RS limits. Cue starts stay fixed.
    {:else}Preview inward trims around scene cuts.{/if}
    Text and tags stay intact. Accepting all safe changes across all pages creates
    one Undo step.
  </p>
  {#if subtitles.dirty}
    <section class="draft" aria-label="Unapplied cue draft">
      <p>Apply or discard the current cue draft before creating a proposal.</p>
      <button
        disabled={!session.canStart}
        onclick={() => void session.applyDraft()}>Apply cue draft</button
      >
      <button disabled={!session.canStart} onclick={() => subtitles.revert()}
        >Discard cue draft</button
      >
      <button disabled={!session.canClose} onclick={onclose}
        >Cancel {isDuration ? 'duration' : 'scene'} review</button
      >
    </section>
  {/if}
  {#if isDuration}
    <label
      >Repair scope
      <select
        aria-label="Repair scope"
        bind:value={scope}
        disabled={!session.canStart}
      >
        <option value="selected">Selected cue</option>
        <option value="document">Whole document</option>
      </select>
    </label>
    <p>
      {scope === 'document'
        ? 'All cues with duration or speed findings will be reviewed.'
        : `Review cue ${subtitles.snapshot.cues.findIndex((c) => c.id === subtitles.selected) + 1}. Other cues remain protected by whole-document validation.`}
    </p>
  {/if}
  <div class="actions">
    <button
      bind:this={preview}
      disabled={!session.canStart ||
        subtitles.dirty ||
        !subtitles.snapshot.cues.length ||
        (isDuration && scope === 'selected' && !subtitles.cue)}
      onclick={() => void session.start()}
      >{view
        ? 'Preview again'
        : isDuration
          ? 'Create duration preview'
          : 'Create scene preview'}</button
    >
    {#if session.running}<button
        disabled={session.pending || session.accepting}
        onclick={() => void session.cancel()}>Cancel proposal</button
      >{/if}
  </div>
  <p role="status">
    {#if session.accepting}Applying the displayed timings…
    {:else if session.pending}Working…
    {:else if view?.phase === 'preparing'}Checking candidate timings…
    {:else if view?.phase === 'cancelling'}Cancelling proposal…
    {:else if view?.phase === 'cancelled'}Proposal cancelled. Subtitles are
      unchanged.
    {:else if view?.phase === 'stale'}The document or quality profile changed.
      Create a new preview.
    {:else if view?.phase === 'failed'}The proposal could not be completed.
    {:else if view && !session.current}Selection, scope or workspace changed.
      Create a new preview.
    {:else if view?.phase === 'reviewing'}{view.changed} safe changes · {view.skipped}
      skipped candidates
    {:else}No timings have changed.{/if}
  </p>
  {#if session.error}<p class="error" role="alert">{session.error}</p>{/if}
  {#if view?.phase === 'reviewing'}
    <p>
      <strong>{view.profile.name}</strong>
      {#if isDuration}
        · {'selectedCueId' in view && view.selectedCueId !== null
          ? 'Selected cue'
          : 'Whole document'} · fixed cue starts{/if}
      · scene buffers {view.profile.timing.sceneBeforeMs} ms before / {view
        .profile.timing.sceneAfterMs} ms after
    </p>
    <p>
      Quality findings: {view.beforeFindings} before → {view.afterFindings} after.
      Skipped candidates stay unchanged.
    </p>
    <details>
      <summary
        >Policy and safety checks{view.profile.limitations.length
          ? ' · Partial profile coverage'
          : ''}</summary
      >
      <p>
        {#if isDuration}Ends can extend or shorten to the nearest permitted
          millisecond. No cue starts move. CPS and duration limits are
          inclusive; the RS maximum is exclusive. Conflicting limits and coupled
          timing changes can require manual review.
        {:else}Starts move later and ends earlier only within the original
          interval.{/if}
        Duration, reading speed, overlap, gap and scene constraints are checked on
        the complete proposed document. Ambiguous overlaps and timing-sensitive or
        unknown tags require manual review.
      </p>
      <p>
        Counting: {view.profile.textAlgorithm} · reading speed: {view.profile
          .speedBasis}. Scene endpoint buffers: {view.profile.timing
          .sceneBeforeMs} ms before / {view.profile.timing.sceneAfterMs} ms after;
        exact boundaries are allowed.
      </p>
      {#if isDuration}
        <p>
          Duration: {view.profile.rules.duration.enabled
            ? `${view.profile.rules.duration.minimumMs}–${view.profile.rules.duration.maximumMs} ms`
            : 'disabled'}. Speed checking: {view.profile.rules.speed.enabled
            ? 'enabled'
            : 'disabled'}. CPS minimum {view.profile.rules.speed.minimumCps ??
            'none'}, maximum {view.profile.rules.speed.maximumCps ?? 'none'}; RS
          minimum {view.profile.rules.speed.minimumRs ?? 'none'}, exclusive
          maximum {view.profile.rules.speed.maximumRsExclusive ?? 'none'}.
          Minimum gap {view.profile.timing.cueGapMs} ms.
        </p>
      {/if}
      {#if view.profile.limitations.length}<strong
          >Partial profile coverage</strong
        >
        <ul>
          {#each view.profile.limitations as limitation, index (index)}<li>
              {limitation}
            </li>{/each}
        </ul>{/if}
    </details>
    <p class="coverage">{sceneCoverage(view.coverage)}</p>
    {#if !view.total && ['checked', 'reduced', 'notRequired'].includes(view.coverage ?? '')}
      <p>
        No {isDuration ? 'duration or reading-speed' : 'scene endpoint'} repairs are
        needed in this scope.
      </p>
    {:else if view.total && !view.changed}
      <p>No candidate can be changed safely under this profile.</p>
    {/if}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Scrollable timing review supports keyboard scrolling.) -->
    <div
      class="table-scroll"
      tabindex="0"
      role="region"
      aria-label={isDuration
        ? 'Duration and speed proposal'
        : 'Scene timing proposal'}
    >
      <table>
        <thead
          ><tr
            ><th>Cue</th><th>Timing before → after</th><th>Reading metrics</th
            ><th>Decision</th></tr
          ></thead
        >
        <tbody
          >{#each view.rows as row (row.cueId)}
            <tr class:skipped={row.skipped !== null}>
              <td>{row.cueIndex + 1}</td>
              <td>
                Start {time(row.before.data.startMs)} → {time(
                  row.after.data.startMs,
                )}
                <br />End {time(row.before.data.endMs)} → {time(
                  row.after.data.endMs,
                )}
                <small
                  >Duration {row.before.data.durationMs} → {row.after.data
                    .durationMs} ms</small
                >
              </td>
              <td
                >CPS {rate(row.before.data.cps)} → {rate(row.after.data.cps)}<br
                />RS {rate(row.before.data.rs)} → {rate(row.after.data.rs)}
              </td>
              <td
                >{row.explanation ??
                  (isDuration
                    ? (row.after.data.endMs ?? 0) > (row.before.data.endMs ?? 0)
                      ? 'Extend to permitted duration/speed'
                      : 'Shorten to permitted duration/speed'
                    : 'Trim to scene buffer edges')}</td
              >
            </tr>
          {/each}</tbody
        >
      </table>
    </div>
    {#if view.total > 100}<nav
        aria-label={isDuration
          ? 'Duration proposal pages'
          : 'Scene proposal pages'}
      >
        <button
          disabled={!session.canStart || !view.offset}
          onclick={() => void session.page(Math.max(0, view.offset - 100))}
          >Previous candidates</button
        >
        <span
          >{view.offset + 1}–{view.offset + view.rows.length} of {view.total}</span
        >
        <button
          disabled={!session.canStart ||
            view.offset + view.rows.length >= view.total}
          onclick={() => void session.page(view.offset + 100)}
          >Next candidates</button
        >
      </nav>{/if}
  {/if}
  <p>
    Saving remains a separate action. Existing recovery or resynchronization
    Save As protection is preserved.
  </p>
  <div class="actions end">
    <button disabled={!session.canClose} onclick={onclose}
      >{session.running
        ? 'Cancel and close'
        : isDuration
          ? 'Close duration review'
          : 'Close scene review'}</button
    >
    <button disabled={!session.canAccept} onclick={() => void accept()}
      >{isDuration ? 'Accept duration changes' : 'Accept scene changes'}</button
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
  p {
    font-size: 13px;
    line-height: 1.5;
  }
  .actions,
  nav {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .end {
    justify-content: flex-end;
    margin-top: 16px;
  }
  .draft,
  .coverage {
    padding: 10px;
    background: #344d5d;
    border: 1px solid #778b90;
    margin-block: 12px;
  }
  .error {
    color: #ffb3ad;
  }
  details {
    font-size: 12px;
    margin-block: 10px;
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
  }
  th,
  td {
    border-bottom: 1px solid #496270;
    text-align: left;
    padding: 8px;
    vertical-align: top;
  }
  th {
    position: sticky;
    top: 0;
    background: #263e4d;
  }
  td:nth-child(2) {
    font-variant-numeric: tabular-nums;
    min-width: 190px;
  }
  small {
    display: block;
    color: #bfd1dc;
    margin-top: 4px;
  }
  .skipped {
    background: #403a31;
  }
</style>
