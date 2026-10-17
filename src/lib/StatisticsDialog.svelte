<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import type { DocumentSession } from './document-session.svelte';
  import type { StatisticsSummary } from './client';
  import { StatisticsSession } from './statistics-session.svelte';
  import { statisticsScope } from './statistics-scope';
  import { qualityRules, sceneCoverage, rating } from './quality-display';
  let {
    subtitles,
    mediaSession,
    context,
    onclose,
    onnavigate,
  }: {
    subtitles: DocumentSession;
    mediaSession: () => number;
    context: () => string;
    onclose: () => void;
    onnavigate: (id: string) => void;
  } = $props();
  let scope = $state<'document' | 'range'>('document');
  let first = $state('1');
  let last = $state(untrack(() => String(subtitles.snapshot.cues.length || 1)));
  let excluded = $state('');
  const input = $derived({
    ...statisticsScope(scope, first, last, excluded),
    key: JSON.stringify([scope, first, last, excluded]),
  });
  const session = untrack(
    () => new StatisticsSession(subtitles, mediaSession, context, () => input),
  );
  const view = $derived(session.view);
  const summary = $derived(
    session.current && view?.phase === 'ready' ? view.summary : null,
  );
  const metrics = $derived(
    summary
      ? ([
          ['Duration (ms)', summary.durationMs],
          ['CPS', summary.cps],
          ['RS', summary.rs],
          ['Lines per cue', summary.lineCount],
          ['Longest line per cue (UTF-16 units)', summary.longestLine],
          ['Original-neighbor gap (ms)', summary.gapsMs],
        ] as [string, StatisticsSummary['durationMs']][])
      : [],
  );
  const unavailable = $derived(
    summary
      ? ([
          ['CPS', summary.unavailableCps],
          ['RS', summary.unavailableRs],
        ] as [string, StatisticsSummary['unavailableCps']][])
      : [],
  );
  const number = (n: number | null | undefined) =>
    n == null ? 'unavailable' : Number(n.toFixed(3)).toLocaleString();
  let dialog: HTMLDialogElement;
  let calculate: HTMLButtonElement;
  function navigate(id: string) {
    if (summary && !session.pending) onnavigate(id);
  }
  onMount(() => {
    dialog.showModal();
    calculate.focus();
    void session.start();
    const timer = setInterval(() => void session.poll(), 400);
    return () => {
      clearInterval(timer);
      session.dispose();
    };
  });
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="statistics-title"
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
>
  <header>
    <h2 id="statistics-title">Document statistics</h2>
    <button onclick={onclose}>Close statistics</button>
  </header>
  <p>
    Statistics describe accepted subtitles. Unapplied cue drafts stay unchanged.
    Choose a cue link to return to the editor; navigation protects any draft.
  </p>
  {#if subtitles.dirty}<p class="notice">
      The current cue has an unapplied draft. It is not included in these
      statistics.
    </p>{/if}
  <div class="scope">
    <label
      >Scope <select aria-label="Statistics scope" bind:value={scope}
        ><option value="document">Whole document</option><option value="range"
          >Cue range</option
        ></select
      ></label
    >
    {#if scope === 'range'}<label
        >First cue <input
          aria-label="First statistics cue"
          type="text"
          inputmode="numeric"
          bind:value={first}
          maxlength="6"
        /></label
      ><label
        >Last cue <input
          aria-label="Last statistics cue"
          type="text"
          inputmode="numeric"
          bind:value={last}
          maxlength="6"
        /></label
      >{/if}
    <label class="exclusions"
      >Exclude cue numbers <input
        aria-label="Excluded statistics cues"
        bind:value={excluded}
        maxlength="700000"
        placeholder="For example: 2, 5, 8"
      /></label
    >
    <button
      bind:this={calculate}
      disabled={!session.canStart}
      onclick={() => void session.start()}>Calculate statistics</button
    >
    {#if session.running}<button
        disabled={session.pending}
        onclick={() => void session.cancel()}>Cancel statistics</button
      >{/if}
  </div>
  <p class="hint">
    Cue numbers are one-based document positions. Range endpoints are inclusive.
    Exclusions must fall inside the scope.
  </p>
  {#if input.error || session.error}<p role="alert">
      {input.error || session.error}
    </p>{/if}
  <p role="status">
    {session.pending || view?.phase === 'preparing'
      ? 'Calculating…'
      : view?.phase === 'cancelling'
        ? 'Cancelling…'
        : view?.phase === 'cancelled'
          ? 'Statistics cancelled.'
          : view?.phase === 'stale'
            ? 'Document or profile changed; calculate again.'
            : view?.phase === 'failed'
              ? 'Statistics could not be calculated.'
              : view && !session.current
                ? 'Scope or workspace changed; calculate again.'
                : summary
                  ? `${summary.includedCues} included cues · ${summary.findingsCount} findings`
                  : 'No current report.'}
  </p>
  {#if summary}
    <p>
      <strong>{summary.profile.name}</strong> · revision {summary.profile
        .revision}
    </p>
    <p class="notice">{sceneCoverage(summary.sceneCoverage)}</p>
    {#if !summary.complete}<p class="notice">
        Quality coverage is partial. Unchecked rules are not passing results.
      </p>{/if}
    <div class="counts" aria-label="Statistics population">
      <span>Total: {summary.totalCues}</span><span
        >In scope: {summary.scopeCues}</span
      ><span>Outside scope: {summary.outsideScopeCues}</span><span
        >Excluded: {summary.excludedCues}</span
      ><strong>Included: {summary.includedCues}</strong><span
        >Empty text: {summary.emptyTextCues}</span
      >
    </div>
    <div class="table-scroll">
      <table aria-label="Metric summaries">
        <thead
          ><tr
            ><th>Metric</th><th>Samples</th><th>Minimum</th><th>Maximum</th><th
              >Mean</th
            ></tr
          ></thead
        ><tbody>
          {#each metrics as [label, value] (label)}<tr
              ><th>{label}</th><td>{value.count}</td><td
                >{#if value.minimum}<button
                    disabled={session.pending}
                    onclick={() => navigate(value.minimum!.cueId)}
                    title={`Go to cue ${value.minimum.cueIndex + 1}`}
                    >{number(value.minimum.value)} · cue {value.minimum
                      .cueIndex + 1}</button
                  >{:else}unavailable{/if}</td
              ><td
                >{#if value.maximum}<button
                    disabled={session.pending}
                    onclick={() => navigate(value.maximum!.cueId)}
                    title={`Go to cue ${value.maximum.cueIndex + 1}`}
                    >{number(value.maximum.value)} · cue {value.maximum
                      .cueIndex + 1}</button
                  >{:else}unavailable{/if}</td
              ><td>{number(value.mean)}</td></tr
            >{/each}
        </tbody>
      </table>
    </div>
    <p>
      Each mean weights its finite samples equally. Line summaries describe each
      cue's line count and longest line. Tied extrema link to the first cue in
      document order.
    </p>
    {#each unavailable as [label, counts] (label)}
      <p>
        {label} unavailable: {counts.total} — {counts.emptyText} empty text, {counts.invalidDuration}
        invalid reading duration, {counts.readingAllowance} reading allowance.
      </p>
    {/each}
    <h3>Reading-speed distribution</h3>
    <p>
      Denominator: {summary.rs.count} cues with finite RS. Unavailable rates are excluded
      from percentages.
    </p>
    <div class="table-scroll">
      <table aria-label="Reading-speed distribution">
        <thead><tr><th>Category</th><th>Cues</th><th>Percentage</th></tr></thead
        ><tbody
          >{#each summary.ratings as bucket (bucket.rating)}<tr
              ><th>{rating(bucket.rating)}</th><td>{bucket.count}</td><td
                >{bucket.percentage === null
                  ? 'unavailable'
                  : `${number(bucket.percentage)}%`}</td
              ></tr
            >{/each}</tbody
        >
      </table>
    </div>
    <h3>Gaps and overlaps</h3>
    <p>
      Original consecutive pairs: {summary.gapsMs.count} included, {summary.excludedNeighborPairs}
      excluded. {summary.negativeGaps} negative gaps · {summary.touchingGaps} touching
      · {summary.positiveGaps} positive gaps. Both cues must be included; exclusions
      never create new neighbors.
    </p>
    <p>
      All included intervals: <strong
        >{summary.overlappingPairs} overlapping pairs · {summary.overlappingCues}
        overlapping cues</strong
      >. Includes nested and equal-start intervals; touching endpoints do not
      overlap.
    </p>
    <details>
      <summary>Counting and coverage</summary>
      <p>
        {summary.profile.textAlgorithm} · {summary.profile.speedBasis}. Reading
        time uses full-document context before filtering. Text counting
        preserves the checker's tag, UTF-16 and line-break policy. Empty text
        still contributes to duration and line summaries; the text “0” is not
        empty.
      </p>
      <ul>
        {#each summary.profile.limitations as limitation, index (index)}<li>
            {limitation}
          </li>{/each}
      </ul>
    </details>
    <h3>Quality outliers</h3>
    <p>
      {summary.findingsCount} findings · {summary.violations} violations · {summary.advisories}
      advisories · {summary.affectedCues} included affected cues. Findings are selected
      by their primary cue; related cues may be outside scope.
    </p>
    <div class="rule-counts">
      {#each summary.rules as rule (rule.rule)}<span
          >{qualityRules.find((r) => r.id === rule.rule)?.label}: {rule.count}</span
        >{/each}
    </div>
    {#if !summary.findingsCount}<p>
        No findings in this scope under the checked rules.
      </p>{/if}
    <div class="findings" aria-label="Statistics outliers">
      {#each view?.findings ?? [] as finding, index (`${view?.id}/${view?.offset}/${index}`)}<div
          class="finding"
        >
          <button
            disabled={session.pending}
            onclick={() => navigate(finding.cueId)}
            >Cue {finding.cueIndex + 1} · {qualityRules.find(
              (r) => r.id === finding.rule,
            )?.label}</button
          ><span
            >{finding.severity} · {number(finding.measured)} · {finding.expected}</span
          >{#if finding.relatedCueId}<button
              disabled={session.pending}
              onclick={() => navigate(finding.relatedCueId!)}
              >Related cue (context)</button
            >{/if}<small>{finding.explanation}</small>
        </div>{/each}
    </div>
    {#if view && summary.findingsCount > 100}<nav aria-label="Statistics pages">
        <button
          disabled={session.pending || !view.offset}
          onclick={() => void session.page(Math.max(0, view.offset - 100))}
          >Previous findings</button
        ><span
          >{view.offset + 1}–{view.offset + view.findings.length} of {summary.findingsCount}</span
        ><button
          disabled={session.pending ||
            view.offset + view.findings.length >= summary.findingsCount}
          onclick={() => void session.page(view.offset + 100)}
          >Next findings</button
        >
      </nav>{/if}
  {/if}
</dialog>

<style>
  dialog {
    color: #e7eef3;
    background: #1e303d;
    border: 1px solid #6c8b9d;
    border-radius: 8px;
    width: min(1060px, 92vw);
    max-height: 90dvh;
    overflow: auto;
    padding: 20px;
    font-size: 13px;
    line-height: 1.5;
  }
  dialog::backdrop {
    background: #0009;
  }
  header,
  .scope,
  .counts,
  nav,
  .rule-counts {
    display: flex;
    align-items: center;
    gap: 10px 16px;
    flex-wrap: wrap;
  }
  header {
    justify-content: space-between;
  }
  h2 {
    margin: 0;
  }
  label {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
  }
  input[inputmode] {
    width: 70px;
  }
  .exclusions {
    flex: 1;
  }
  .exclusions input {
    min-width: 150px;
    flex: 1;
  }
  .hint {
    color: #b9cdd8;
  }
  .notice {
    padding: 8px;
    background: #344d5d;
    border: 1px solid #778b90;
  }
  [role='alert'] {
    color: #ffb3ad;
  }
  .counts {
    padding-block: 8px;
  }
  .table-scroll {
    overflow: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: 7px;
    border-bottom: 1px solid #496270;
  }
  th {
    font-weight: 600;
  }
  .finding {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    padding-block: 7px;
    border-top: 1px solid #496270;
  }
  .finding small {
    flex-basis: 100%;
    color: #bfd3df;
  }
  .findings {
    max-height: 34vh;
    overflow: auto;
    margin-block: 10px;
  }
  nav {
    margin-block: 10px;
  }
</style>
