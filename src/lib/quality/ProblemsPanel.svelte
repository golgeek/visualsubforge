<script lang="ts">
  import { tick } from 'svelte';
  import { settled } from '../ui/settled.svelte';
  import { countLabel } from '../ui/count-label';
  import { qualityText } from './quality-display';
  import type { QualitySession } from './quality-session.svelte';
  import type { QualityRequest } from '../client';
  import {
    changeLanguageWords,
    type LanguageWordChange,
    type TextOperation,
    type QualityView,
  } from '../client';
  import { qualityRules, sceneCoverage } from './quality-display';
  let {
    session,
    selected = null,
    onfixtiming,
    documentLoaded = true,
    disabled,
    onnavigate,
    onclose,
    onlanguage,
  }: {
    session: QualitySession;
    selected?: number | null;
    onfixtiming?: () => void;
    documentLoaded?: boolean;
    disabled: boolean;
    onnavigate: (id: string, origin: HTMLButtonElement) => void;
    onclose: () => void;
    onlanguage: (operation: TextOperation) => void;
  } = $props();
  const view = $derived(session.displayedList ?? session.view);
  const summary = $derived(
    view?.profileHash ? countLabel(view.findingsCount, 'issue') : '',
  );
  const updating = settled(() => session.listStale);
  let findings: HTMLDivElement;
  type Finding = QualityView['findings'][number];
  function findingKey(f: Finding) {
    return JSON.stringify([
      f.cueId,
      f.rule,
      f.relatedCueId,
      f.language?.start,
      f.language?.end,
    ]);
  }
  const findingRows = $derived.by(() => {
    const occurrences: Record<string, number> = {};
    return (view?.findings ?? []).map((finding) => {
      const identity = findingKey(finding);
      const occurrence = occurrences[identity] ?? 0;
      occurrences[identity] = occurrence + 1;
      return { finding, key: identity + '/' + occurrence };
    });
  });
  let followed: string | null = null;
  $effect.pre(() => {
    // Read old row geometry immediately before publishing the replacement page.
    const page = view;
    if (!page || !findings) return;
    const scroll = findings;
    if (scroll.scrollTop === 0) return;
    const bounds = scroll.getBoundingClientRect();
    const rows = () =>
      Array.from(findings.querySelectorAll<HTMLElement>('.finding'));
    const visible = rows().filter((row) => {
      const box = row.getBoundingClientRect();
      return box.top < bounds.bottom && box.bottom > bounds.top;
    });
    const anchor =
      visible.find((row) => row.dataset.key === followed) ?? visible[0];
    if (!anchor) return;
    const offset = anchor.getBoundingClientRect().top - bounds.top;
    const index = Number(anchor.dataset.index);
    let current = true;
    void tick().then(() => {
      if (!current || !scroll.isConnected) return;
      const list = rows();
      const next =
        list.find((row) => row.dataset.key === anchor.dataset.key) ??
        list.reduce<HTMLElement | undefined>((nearest, row) => {
          if (!nearest) return row;
          const distance = Number(row.dataset.index) - index;
          const best = Number(nearest.dataset.index) - index;
          return Math.abs(distance) < Math.abs(best) ||
            (Math.abs(distance) === Math.abs(best) && distance >= 0 && best < 0)
            ? row
            : nearest;
        }, undefined);
      if (next)
        scroll.scrollTop +=
          next.getBoundingClientRect().top -
          scroll.getBoundingClientRect().top -
          offset;
    });
    return () => {
      current = false;
    };
  });
  const usable = $derived(
    view === session.view &&
      !!view?.profileHash &&
      !session.listStale &&
      !disabled,
  );
  let wordPending = $state(false);
  let wordError = $state('');
  const error = $derived(session.error || session.view?.error || wordError);
  async function wordChange(change: LanguageWordChange) {
    if (!view || !usable || wordPending) return;
    wordPending = true;
    wordError = '';
    try {
      await changeLanguageWords(view.key.epoch, change);
      session.restart();
    } catch (e) {
      wordError = String(e);
    } finally {
      wordPending = false;
    }
  }
  function review(finding: QualityView['findings'][number]) {
    const d = finding.language;
    if (!usable || !d || d.manualReason) return;
    const target = {
      cueIndex: finding.cueIndex,
      start: d.start,
      end: d.end,
      original: d.word,
    };
    onlanguage(
      d.replacement === null
        ? { kind: 'languageSuggestions', ...target }
        : { kind: 'languageCorrection', ...target, replacement: d.replacement },
    );
  }
</script>

<section class="problems" aria-label="Subtitle issues">
  <header>
    <div
      class="status-slot"
      data-summary={summary || 'Open subtitles to check for issues'}
    >
      {#if error}<span role="alert" title={error}>{error}</span>
      {:else}<span role="status"
          >{!documentLoaded
            ? 'Open subtitles to check for issues'
            : session.paused
              ? 'Checking paused'
              : !view?.profileHash && session.view?.phase === 'checking'
                ? 'Checking…'
                : updating.showUpdating
                  ? 'Updating…'
                  : session.view?.phase === 'failed'
                    ? 'Check failed'
                    : summary || 'Open subtitles to check'}</span
        >
      {/if}
    </div>
    {#if onfixtiming && view?.timingFindingsCount}<button
        {disabled}
        aria-label="Fix timing issues…"
        onclick={onfixtiming}
        >Fix timing<span class="suffix">&#32;issues</span>…</button
      >{/if}
    <button
      onclick={() => session.restart()}
      disabled={disabled || !documentLoaded}>Check again</button
    >
    <button
      onclick={() => session.pause()}
      disabled={session.paused || disabled || !documentLoaded}
      aria-label="Pause checking"
      >Pause <span class="suffix">checking</span></button
    >
    <button aria-label="Close issues" onclick={onclose}
      >Close<span class="suffix">&#32;issues</span></button
    >
  </header>
  <div class="filters">
    <label
      >Rule <select
        aria-label="Issue rule"
        value={session.rule ?? ''}
        onchange={(e) => {
          session.rule = (e.currentTarget.value ||
            null) as QualityRequest['rule'];
          session.offset = 0;
        }}
      >
        <option value="">All rules</option
        >{#each qualityRules as rule (rule.id)}<option value={rule.id}
            >{rule.label}</option
          >{/each}
      </select></label
    >
    <label
      >Severity <select
        aria-label="Issue severity"
        value={session.severity ?? ''}
        onchange={(e) => {
          session.severity = (e.currentTarget.value ||
            null) as QualityRequest['severity'];
          session.offset = 0;
        }}
        ><option value="">All</option><option value="violation"
          >Violation</option
        ><option value="advisory">Advisory</option></select
      ></label
    >
    <button
      disabled={!usable || session.stale || !view?.previousCue}
      data-nav-key="previousCue"
      onclick={(event) =>
        view?.previousCue && onnavigate(view.previousCue, event.currentTarget)}
      aria-label="Previous cue · ⇧F4"
      title="Previous cue · ⇧F4"><span>Previous cue · ⇧F4</span></button
    >
    <button
      disabled={!usable || session.stale || !view?.nextCue}
      data-nav-key="nextCue"
      onclick={(event) =>
        view?.nextCue && onnavigate(view.nextCue, event.currentTarget)}
      aria-label="Next cue · F4"
      title="Next cue · F4"><span>Next cue · F4</span></button
    >
  </div>
  <div class="findings" bind:this={findings} aria-busy={updating.showUpdating}>
    {#if view}<details>
        <summary>Check details</summary>
        <p class="policy">
          {view.profile.name} · {countLabel(
            view.affectedCues,
            'cue with issues',
            'cues with issues',
          )}
        </p>
        <p class="policy">{sceneCoverage(view.sceneCoverage)}</p>
        {#if view.languageCoverage?.enabled}
          <p class="policy">
            {view.profile.languageTools?.dictionary === 'fr'
              ? 'French Classique 7.7'
              : 'English US SCOWL 2020.12.07'} · {countLabel(
              view.languageCoverage.checkedWords,
              'word',
            )} checked · {countLabel(
              view.languageCoverage.skippedWords,
              'word',
            )}
            and {countLabel(view.languageCoverage.skippedCues, 'cue')} skipped{view
              .languageCoverage.personalized
              ? ' · includes personal/session words'
              : ''}.
          </p>
          <button
            disabled={!usable || wordPending}
            onclick={() => void wordChange({ kind: 'clearIgnored' })}
            >Clear session ignored words</button
          >
        {/if}
        {#if view.profile.limitations.length}<p>
            Partial coverage — unsupported rules are not checked
          </p>
          <ul>
            {#each view.profile.limitations as text, index (index)}<li>
                {text}
              </li>{/each}
          </ul>
        {/if}
        {#if view?.profileHash}<span
            >{countLabel(view.filteredCount, 'matching issue')} · {countLabel(
              view.filteredCues,
              'cue',
            )}</span
          >{/if}
      </details>{/if}
    {#if view?.profileHash}
      {#if !view.filteredCount}<p>
          No findings for these filters.{view.profile.limitations.length
            ? ' Profile coverage remains partial.'
            : ''}
        </p>{/if}
      {#each findingRows as { finding, key } (key)}
        <div
          class="finding"
          data-key={key}
          data-index={finding.cueIndex}
          aria-current={String(selected) === finding.cueId ? 'true' : undefined}
        >
          <button
            {disabled}
            data-nav-key={'issue-' + key}
            onclick={(event) => {
              followed = key;
              onnavigate(finding.cueId, event.currentTarget);
            }}
            >Cue {finding.cueIndex + 1} · {qualityRules.find(
              (r) => r.id === finding.rule,
            )?.label}</button
          >
          <span
            >{finding.severity} · measured {finding.measured === null
              ? 'unavailable'
              : Number(finding.measured.toFixed(3))} · {qualityText(
              finding.expected,
            )}</span
          >
          {#if finding.relatedCueId}<button
              {disabled}
              data-nav-key={'related-' + key}
              onclick={(event) =>
                onnavigate(finding.relatedCueId!, event.currentTarget)}
              >Related cue</button
            >{/if}
          <small>{qualityText(finding.explanation)}</small>
          {#if finding.language}
            <span>“{finding.language.word}”</span>
            {#if finding.language.manualReason}<small
                >{finding.language.manualReason}</small
              >
            {:else}<button
                disabled={!usable || wordPending}
                onclick={() => review(finding)}
                >{finding.rule === 'spelling'
                  ? 'Suggestions'
                  : 'Review correction'}</button
              >{/if}
            {#if finding.rule === 'spelling'}
              <button
                disabled={!usable || wordPending}
                onclick={() =>
                  void wordChange({
                    kind: 'ignore',
                    language: view.profile.languageTools?.dictionary ?? 'en-US',
                    word: finding.language!.word,
                  })}>Ignore word this session</button
              >
              <button
                disabled={!usable || wordPending}
                onclick={() =>
                  void wordChange({
                    kind: 'remember',
                    language: view.profile.languageTools?.dictionary ?? 'en-US',
                    word: finding.language!.word,
                  })}>Add personal word</button
              >
            {/if}
          {/if}
        </div>
      {/each}
    {/if}
    {#if view && view.filteredCount > 100}<div class="paging">
        <button
          disabled={!usable || !view.offset}
          onclick={() => (session.offset = Math.max(0, view.offset - 100))}
          >Previous page</button
        >
        <span
          >{`${view.offset + 1}–${Math.min(view.offset + 100, view.filteredCount)} of ${view.filteredCount}`}</span
        >
        <button
          disabled={!usable || view.offset + 100 >= view.filteredCount}
          onclick={() => (session.offset = view.offset + 100)}>Next page</button
        >
      </div>{/if}
  </div>
</section>

<style>
  .problems {
    display: flex;
    flex-direction: column;
    height: 100%;
    box-sizing: border-box;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    padding: 3px 6px;
    border-left: 1px solid var(--line);
    background: var(--surface-raised);
    font-size: 12px;
  }
  header,
  .filters,
  .paging {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    flex: none;
    gap: 2px 4px;
  }

  .status-slot {
    flex: 1 1 auto;
    position: relative;
    display: grid;
    min-width: 0;
  }
  .status-slot span,
  .status-slot::before {
    grid-area: 1 / 1;
  }
  .status-slot::before {
    content: attr(data-summary);
    visibility: hidden;
  }
  .policy {
    margin: 6px 0;
    color: var(--text-muted);
  }
  details {
    margin-block: 0;
  }
  .findings {
    margin-top: 2px;
    flex: 1;
    min-height: 0;
    overflow: auto;
    overflow-anchor: none;
  }
  .finding {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px 8px;
    border-top: 1px solid var(--line);
    padding-block: 5px;
  }
  .findings[aria-busy='true'] {
    opacity: 0.6;
  }
  .finding[aria-current='true'] {
    background: var(--accent-soft);
  }
  .finding small {
    flex-basis: 100%;
    color: var(--text-muted);
  }
  header button,
  .filters button,
  .filters select {
    padding: 2px 4px;
    font-size: 11px;
  }
  .filters select {
    max-width: 108px;
  }
  .filters label {
    white-space: nowrap;
  }
  .filters [data-nav-key] span {
    position: absolute;
    clip-path: inset(50%);
  }
  .filters [data-nav-key='previousCue']::before {
    content: '‹';
  }
  .filters [data-nav-key='nextCue']::before {
    content: '›';
  }
  .filters [data-nav-key] {
    min-width: 24px;
  }
  summary {
    cursor: pointer;
  }
  .status-slot [role='alert'] {
    position: absolute;
    inset: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  [role='alert'] {
    color: var(--err);
  }
  @container (max-width: 400px) {
    .suffix {
      display: none;
    }
    .filters select {
      max-width: 80px;
    }
  }
</style>
