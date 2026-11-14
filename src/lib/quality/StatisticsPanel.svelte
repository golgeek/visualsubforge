<script lang="ts">
  import { qualityText } from './quality-display';
  import { onMount, untrack } from 'svelte';
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { StatisticsSummary } from '../client';
  import { StatisticsSession } from './statistics-session.svelte';
  import { statisticsSelection } from './statistics-scope';
  import { qualityRules, sceneCoverage, rating } from './quality-display';
  let {
    subtitles,
    mediaSession,
    context,
    onclose,
    onnavigate,
    scopeIds = () => null,
    enabled = () => true,
  }: {
    scopeIds?: () => number[] | null;
    enabled?: () => boolean;
    subtitles: DocumentSession;
    mediaSession: () => number;
    context: () => string;
    onclose: () => void;
    onnavigate: (id: string) => void;
  } = $props();
  const input = $derived({
    ...statisticsSelection(subtitles.snapshot.cues, scopeIds()),
    key: JSON.stringify(scopeIds()),
  });
  const session = untrack(
    () =>
      new StatisticsSession(
        subtitles,
        mediaSession,
        context,
        () => input,
        undefined,
        () => enabled(),
      ),
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
          ['Longest line per cue (characters)', summary.longestLine],
          ['Gap to next cue (ms)', summary.gapsMs],
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
  function navigate(id: string) {
    if (summary && !session.pending) onnavigate(id);
  }
  onMount(() => () => session.dispose());
</script>

<section class="statistics" aria-labelledby="statistics-title">
  <header>
    <h2 id="statistics-title">Document statistics</h2>
    <button onclick={onclose}>Close</button>
  </header>
  <p>
    Statistics describe accepted subtitles. Unapplied cue drafts stay unchanged.
    Choose a cue link to return to the editor; navigation protects any draft.
  </p>
  {#if subtitles.dirty}<p class="notice">
      The current cue has an unapplied draft. It is not included in these
      statistics.
    </p>{/if}
  {#if session.error || view?.phase === 'stale' || view?.phase === 'failed'}<button
      disabled={!session.canStart}
      onclick={() => void session.start()}>Retry statistics</button
    >{/if}
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
    {#if summary.languageCoverage.enabled}<p class="notice">
        Whole-document language coverage: {summary.languageCoverage
          .checkedWords} checked words; {summary.languageCoverage.skippedWords} skipped
        words and {summary.languageCoverage.skippedCues} skipped cues{summary
          .languageCoverage.personalized
          ? '; personal/session words included'
          : ''}.
      </p>{/if}
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
        Reading time includes neighboring cues, even when filtered out. See
        Settings → Subtitle quality → About this profile for counting details.
        Empty text contributes to duration and line summaries.
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
            >{finding.severity} · {number(finding.measured)} · {qualityText(
              finding.expected,
            )}</span
          >{#if finding.relatedCueId}<button
              disabled={session.pending}
              onclick={() => navigate(finding.relatedCueId!)}
              >Related cue (context)</button
            >{/if}<small>{qualityText(finding.explanation)}</small>
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
</section>

<style>
  .statistics {
    color: var(--text);
    background: var(--surface-panel);
    padding: 10px;
    font-size: 12px;
    line-height: 1.5;
  }
  header,
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
  .notice {
    padding: 8px;
    background: var(--surface-raised);
    border: 1px solid var(--line-strong);
  }
  [role='alert'] {
    color: var(--err);
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
    border-bottom: 1px solid var(--line);
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
    border-top: 1px solid var(--line);
  }
  .finding small {
    flex-basis: 100%;
    color: var(--text-muted);
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
