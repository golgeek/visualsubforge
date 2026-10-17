<script lang="ts">
  import type { QualitySession } from './quality-session.svelte';
  import type { QualityRequest } from './client';
  import { qualityRules } from './quality-display';
  let {
    session,
    disabled,
    pending,
    onnavigate,
    onresolve,
    onclose,
  }: {
    session: QualitySession;
    disabled: boolean;
    pending: boolean;
    onnavigate: (id: string) => void;
    onresolve: (choice: 'apply' | 'discard' | 'cancel') => void;
    onclose: () => void;
  } = $props();
  const view = $derived(session.view);
  const usable = $derived(!!view?.profileHash && !session.stale && !disabled);
</script>

<section class="problems" aria-label="Subtitle Problems">
  <header>
    <strong>Problems</strong>
    <span role="status"
      >{session.paused
        ? 'Checking paused'
        : view?.phase === 'checking' || view?.phase === 'cancelling'
          ? 'Checking…'
          : session.stale
            ? 'Updating…'
            : view?.phase === 'failed'
              ? 'Check failed'
              : view?.profileHash
                ? `${view.findingsCount} findings · ${view.affectedCues} affected cues`
                : 'Open subtitles to check'}</span
    >
    <button onclick={() => session.restart()} {disabled}>Check again</button>
    <button
      onclick={() => session.pause()}
      disabled={session.paused || disabled}>Pause checking</button
    >
    <button onclick={onclose}>Close Problems</button>
  </header>
  {#if session.error || view?.error}<p role="alert">
      {session.error || view?.error}
    </p>{/if}
  {#if view}
    <p class="policy">
      {view.profile.name} · {view.profile.textAlgorithm} · {view.profile
        .speedBasis}
    </p>
    {#if view.profile.limitations.length}<details>
        <summary>Partial coverage — unsupported rules are not checked</summary>
        <ul>
          {#each view.profile.limitations as text, index (index)}<li>
              {text}
            </li>{/each}
        </ul>
      </details>{/if}
  {/if}
  <div class="filters">
    <label
      >Rule <select
        aria-label="Problem rule"
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
        aria-label="Problem severity"
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
      disabled={!usable || !view?.previousCue}
      onclick={() => view?.previousCue && onnavigate(view.previousCue)}
      >Previous cue · ⇧F4</button
    >
    <button
      disabled={!usable || !view?.nextCue}
      onclick={() => view?.nextCue && onnavigate(view.nextCue)}
      >Next cue · F4</button
    >
    {#if view?.profileHash}<span
        >{view.filteredCount} matching findings · {view.filteredCues} cues</span
      >{/if}
  </div>
  {#if pending}<div
      class="draft-choice"
      role="group"
      aria-label="Resolve cue draft before navigation"
    >
      <span>The current cue has an unapplied draft.</span>
      <button {disabled} onclick={() => onresolve('apply')}
        >Apply draft and navigate</button
      >
      <button {disabled} onclick={() => onresolve('discard')}
        >Discard draft and navigate</button
      >
      <button {disabled} onclick={() => onresolve('cancel')}
        >Cancel navigation</button
      >
    </div>{/if}
  <div class="findings" aria-busy={session.stale}>
    {#if view?.profileHash && !session.stale}
      {#if !view.filteredCount}<p>
          No findings for these filters.{view.profile.limitations.length
            ? ' Profile coverage remains partial.'
            : ''}
        </p>{/if}
      {#each view.findings as finding, index (`${view.id}/${view.offset + index}`)}
        <div class="finding">
          <button disabled={!usable} onclick={() => onnavigate(finding.cueId)}
            >Cue {finding.cueIndex + 1} · {qualityRules.find(
              (r) => r.id === finding.rule,
            )?.label}</button
          >
          <span
            >{finding.severity} · measured {finding.measured === null
              ? 'unavailable'
              : Number(finding.measured.toFixed(3))} · {finding.expected}</span
          >
          {#if finding.relatedCueId}<button
              disabled={!usable}
              onclick={() => onnavigate(finding.relatedCueId!)}
              >Related cue</button
            >{/if}
          <small>{finding.explanation}</small>
        </div>
      {/each}
    {/if}
  </div>
  {#if view && view.filteredCount > 100}<div class="paging">
      <button
        disabled={!usable || !view.offset}
        onclick={() => (session.offset = Math.max(0, view.offset - 100))}
        >Previous page</button
      >
      <span
        >{view.offset + 1}–{Math.min(view.offset + 100, view.filteredCount)} of {view.filteredCount}</span
      >
      <button
        disabled={!usable || view.offset + 100 >= view.filteredCount}
        onclick={() => (session.offset = view.offset + 100)}>Next page</button
      >
    </div>{/if}
</section>

<style>
  .problems {
    flex: 0 0 42%;
    min-width: 0;
    min-height: 0;
    overflow: auto;
    padding: 8px;
    border-left: 1px solid #537080;
    background: #213744;
    font-size: 12px;
  }
  header,
  .filters,
  .draft-choice,
  .paging {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 10px;
  }
  header strong {
    color: #a2e6d9;
  }
  header span {
    flex: 1;
  }
  .policy {
    margin: 6px 0;
    color: #bfd3df;
  }
  details {
    margin-block: 5px;
  }
  .findings {
    margin-top: 6px;
  }
  .finding {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px 8px;
    border-top: 1px solid #415763;
    padding-block: 5px;
  }
  .finding small {
    flex-basis: 100%;
    color: #bfd3df;
  }
  .draft-choice {
    background: #344f5d;
    padding: 6px;
    margin-block: 6px;
  }
  [role='alert'] {
    color: #ffb3ad;
  }
  @media (max-width: 999px) {
    .problems {
      flex-basis: 45%;
    }
  }
</style>
