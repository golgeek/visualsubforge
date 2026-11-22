<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    timingFixView,
    type TimingFixOptions,
    type TimingFixInput,
    type TimingFamilies,
    type TimingFixView,
  } from '../client';
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { WorkspaceView } from '../workspace/workspace-view.svelte';
  import { TimingFixSession } from './timing-fix-session.svelte';
  import { timingGhosts, type TimingPreview } from './review-preview';
  let {
    view,
    subtitles,
    mediaSession,
    scopeIds,
    previewIds,
    context,
    enabled,
    options,
    onoptions,
    onbusy,
    onpreview,
    onaccepted,
    onnavigate,
  }: {
    view: WorkspaceView;
    subtitles: DocumentSession;
    mediaSession: () => number;
    scopeIds: () => number[] | null;
    previewIds: () => readonly number[];
    context: () => string;
    enabled: () => boolean;
    options: TimingFixOptions;
    onoptions: (o: TimingFixOptions) => void;
    onbusy: (v: boolean) => void;
    onpreview: (v: TimingPreview | null) => void;
    onaccepted: () => void;
    onnavigate: (id: string) => void;
  } = $props();
  const labels: Record<keyof TimingFamilies, string> = {
    gaps: 'Gaps and overlaps',
    speed: 'Duration and reading speed',
    shots: 'Shot changes',
  };
  const familyKeys = ['gaps', 'speed', 'shots'] as const;
  let toggled = $state<number[]>([]);
  let selectionKey = $state('');
  let openRows = $state<number[]>([]);
  let manualOpen = $state(true);
  let manualOffset = $state(0);
  let manualPage = $state.raw<TimingFixView | null>(null);
  let manualError = $state('');
  const semantic = $derived(
    JSON.stringify([
      subtitles.snapshot.generation,
      subtitles.snapshot.revision,
      mediaSession(),
      scopeIds(),
      context(),
      view.families,
      options,
    ]),
  );
  const input = (): TimingFixInput => ({
    options: { ...options },
    families: { ...view.families },
    scope: scopeIds(),
    toggled: selectionKey === semantic ? toggled : [],
    omitTrades: false,
  });
  const session = untrack(
    () =>
      new TimingFixSession(subtitles, mediaSession, input, {
        enabled,
        selection: () => null,
        context,
        previewIds,
      }),
  );
  const result = $derived(session.current ? session.view : null);
  const coverage = $derived(result?.coverage);
  const blocked = $derived(
    coverage === 'partial' || coverage === 'unavailable',
  );
  const unavailable = $derived(coverage === 'noMedia' || blocked);
  const pageId = $derived(result?.phase === 'reviewing' ? result.id : null);
  $effect(() => {
    const id = pageId,
      offset = manualOffset;
    let current = true;
    manualPage = null;
    manualError = '';
    if (id !== null)
      void timingFixView(id, offset, 100, 'manual').then(
        (v) => {
          if (current) manualPage = v;
        },
        (e) => {
          if (current) manualError = String(e);
        },
      );
    return () => {
      current = false;
    };
  });
  // Drafts retire the proposal. Keep only navigation targets from this document.
  let lastManual = $state.raw<{
    key: string;
    rows: TimingFixView['manual'];
  } | null>(null);
  const documentKey = $derived(
    subtitles.snapshot.generation + '/' + subtitles.snapshot.revision,
  );
  $effect(() => {
    if (manualPage?.phase === 'reviewing')
      lastManual = { key: documentKey, rows: manualPage.manual };
  });
  function prepareSelection() {
    if (selectionKey !== semantic) {
      toggled = [];
      selectionKey = semantic;
    }
  }
  function toggle(index: number) {
    prepareSelection();
    toggled = [...toggled, index];
  }
  function untickTrades() {
    prepareSelection();
    toggled = [...toggled, ...(result?.withoutTradesToggles ?? [])];
  }
  function setPolicy(patch: Partial<TimingFixOptions>) {
    onoptions({ ...options, ...patch });
  }
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
  const chip = (rule: string) =>
    rule === 'gap' || rule === 'overlap'
      ? 'Gap'
      : rule.startsWith('scene')
        ? 'Shot'
        : rule.startsWith('duration')
          ? 'Duration'
          : 'Speed';
</script>

<section class="timing-review" aria-label="Live timing review">
  <div class="scroll">
    {#if view.policyOpen}
      <div class="policy">
        <span>If rules conflict</span>
        <div class="switch" role="radiogroup" aria-label="If rules conflict">
          {#each ['readingSpeed', 'spacing'] as const as priority (priority)}
            <label
              ><input
                type="radio"
                name="timing-priority"
                checked={options.priority === priority}
                onchange={() => setPolicy({ priority })}
              />{priority === 'readingSpeed'
                ? 'Keep reading speed'
                : 'Keep spacing'}</label
            >
          {/each}
        </div>
        <label for="timing-limit">Start moves</label><span
          ><input
            id="timing-limit"
            aria-label="Start move limit in milliseconds"
            type="number"
            min="0"
            max="1000"
            step="1"
            value={options.startToleranceMs}
            oninput={(e) => {
              const n = e.currentTarget.valueAsNumber;
              if (Number.isInteger(n) && n >= 0 && n <= 1000)
                setPolicy({ startToleranceMs: n });
            }}
          /> ms · 0 = off</span
        >
        <label for="timing-edges">Shot changes</label><select
          id="timing-edges"
          aria-label="Shot change edges"
          value={options.sceneEdges}
          onchange={(e) =>
            setPolicy({
              sceneEdges:
                e.currentTarget.value === 'nearest' ? 'nearest' : 'inward',
            })}
          ><option value="inward">Move inward only</option><option
            value="nearest">Nearest allowed edge</option
          ></select
        >
        <small
          >{options.priority === 'spacing'
            ? 'Gaps and shot changes win. A faster cue is marked as a trade-off.'
            : 'Reading speed is protected. A gap or shot fix that would make a cue too fast stays manual.'}
          A start can move inward off a cut even with start moves off.</small
        >
      </div>
    {/if}
    <div class="rules" role="group" aria-label="Rules to fix">
      <p>Fix these rules. The others are protected and never made worse.</p>
      {#each familyKeys as family (family)}<label
          ><input
            type="checkbox"
            checked={view.families[family] &&
              !(family === 'shots' && unavailable)}
            disabled={session.accepting || (family === 'shots' && unavailable)}
            onchange={(e) =>
              (view.families = {
                ...view.families,
                [family]: e.currentTarget.checked,
              })}
          />{labels[family]}
          <span class="number"
            >{#if family === 'shots' && unavailable}{blocked
                ? 'scan incomplete'
                : 'no video'}{:else if result}{result.before[family]} → {view
                .families[family]
                ? result.after[family]
                : result.before[family]}{view.families[family]
                ? ''
                : ' · protected'}{:else}…{/if}</span
          >
        </label>{/each}
    </div>
    {#if blocked}<p class="scan-reason">
        Finish the scene scan to apply timing changes.
      </p>{/if}
    {#if subtitles.dirty}<p>Waiting for the cue edit to commit.</p>
      {#if lastManual?.key === documentKey}{#each lastManual.rows as r (r.cueId)}<div
            class="manual"
          >
            <span>{r.cueNumber}</span><span>{r.detail.lead}</span><button
              onmousedown={(e) => e.preventDefault()}
              onclick={() => onnavigate(r.cueId)}>Edit</button
            >
          </div>{/each}{/if}{/if}
    {#if session.error || session.view?.phase === 'failed' || session.view?.phase === 'stale'}<p
        role="alert"
      >
        {session.error ||
          session.view?.error ||
          'Preview is stale. Check the preview again.'}
      </p>
      <button onclick={() => session.retry()}>Retry preview</button>
    {:else if !result || result.phase !== 'reviewing'}<p role="status">
        {session.accepting ? 'Applying timings…' : 'Updating preview…'}
      </p>
    {:else}
      <div class="heading">
        <strong class="number"
          >{result.before.total} → {result.after.total}</strong
        > findings
      </div>
      {#if result.selectionError}<p role="alert">
          {result.selectionError}
        </p>{/if}{#if result.selectionError || toggled.length}<button
          onclick={() => {
            prepareSelection();
            toggled = [];
          }}>Restore default selection</button
        >{/if}
      <div class="heading">
        <strong>Changes</strong><span class="number">{result.total}</span
        >{#if result.trades}<button data-variant="quiet" onclick={untickTrades}
            >Untick trade-offs ({result.trades})</button
          >{/if}
      </div>
      {#if !result.total}<p>
          No changes for the ticked rules in this scope.
        </p>{/if}
      {#each result.rows as row (row.cueIndex)}
        <div class="change">
          <input
            type="checkbox"
            aria-label={'Include cue ' + row.cueLabel}
            checked={row.selected}
            disabled={session.accepting}
            onchange={() => toggle(row.cueIndex)}
          />
          <button
            class="cue"
            data-variant="quiet"
            aria-label={'Go to cue ' + row.cueLabel}
            onclick={() => onnavigate(row.cueId)}>{row.cueLabel}</button
          >
          <button
            class="detail"
            data-variant="quiet"
            aria-expanded={openRows.includes(row.cueIndex)}
            onclick={() =>
              (openRows = openRows.includes(row.cueIndex)
                ? openRows.filter((i) => i !== row.cueIndex)
                : [...openRows, row.cueIndex])}
          >
            {#each [...new Set(row.unit.fixed.map(chip))] as name (name)}<span
                class="chip">{name}</span
              >{/each}
            {#if row.unit.partial}<span class="chip">Partial</span>{/if}
            {#if row.startMoved}<span class="chip moved">Start moved</span>{/if}
            {#if row.unit.tradeOff}<span class="chip trade">Trade-off</span
              >{/if}<span class="number">{row.delta}</span>{#if row.needs}
              <small>{row.needs}</small>{/if}
            {#if row.replaces}<small>{row.replaces}</small>{/if}
            {#if row.offReason}<small>{row.offReason}</small>{/if}
          </button>
          <small class="effect number"
            >{#each row.unit.effectsText as effect, index (index)}{#if index}
                ·
              {/if}<span class:warning={effect.warning}>{effect.text}</span
              >{/each}</small
          >
          {#if openRows.includes(row.cueIndex)}<small class="times number"
              >{#each row.timecodes as line (line)}<span>{line}</span
                >{/each}</small
            >{/if}
        </div>
      {/each}
      {#if result.total > 100}<nav aria-label="Timing proposal pages">
          <button
            disabled={!result.offset}
            onclick={() => session.page(Math.max(0, result!.offset - 100))}
            >Previous</button
          >{result.offset + 1}–{result.offset + result.rows.length} / {result.total}<button
            disabled={result.offset + 100 >= result.total}
            onclick={() => session.page(result!.offset + 100)}>Next</button
          >
        </nav>{/if}
      {#if result.manualCount}
        <div class="heading">
          <button
            data-variant="quiet"
            aria-expanded={manualOpen}
            onclick={() => (manualOpen = !manualOpen)}
            >{manualOpen ? '▾' : '▸'} Needs manual work {result.manualCount}</button
          >
        </div>
        {#if manualOpen}{#if manualError}<p role="alert">{manualError}</p>{/if}
          {#each manualPage?.manual ?? [] as r (r.cueId)}<div class="manual">
              <span class="number">{r.cueNumber}</span>
              <div>
                <span>{r.detail.lead}</span>
                {#each r.detail.facts as fact (fact)}<small>{fact}</small
                  >{/each}
                {#if r.detail.budgetText}<small class="budget"
                    >{r.detail.budgetText}</small
                  >{/if}
              </div>
              <button onclick={() => onnavigate(r.cueId)}>Edit</button>
            </div>{/each}
          {#if manualPage && result.manualCount > 100}<nav
              aria-label="Manual work pages"
            >
              <button
                disabled={!manualPage.offset}
                onclick={() =>
                  (manualOffset = Math.max(0, manualPage!.offset - 100))}
                >Previous</button
              >{manualPage.offset + 1}–{manualPage.offset +
                manualPage.manual.length} / {result.manualCount}<button
                disabled={manualPage.offset + 100 >= result.manualCount}
                onclick={() => (manualOffset = manualPage!.offset + 100)}
                >Next</button
              >
            </nav>{/if}
        {/if}
      {:else if !result.total}<p>
          No timing findings for the ticked rules in this scope.
        </p>{/if}
    {/if}
  </div>
  <footer>
    {#if coverage === 'notRequired'}<p>
        Scene checks are disabled because both scene buffers are zero.
      </p>{/if}{#if coverage === 'reduced'}<p>
        Reduced detection can miss cuts.
      </p>{/if}{#if coverage === 'noMedia'}<p>
        No video, so shot changes weren’t checked.
      </p>{:else if blocked}<p>
        Finish the scene scan to apply timing changes.
      </p>{/if}
    {#if previewIds().length > 200}<p>
        Preview shows the first 200 visible cues. Apply includes all checked
        changes.
      </p>{/if}
    <button
      data-variant="primary"
      class="apply"
      disabled={!session.canAccept ||
        blocked ||
        (coverage === 'noMedia' &&
          !view.families.gaps &&
          !view.families.speed) ||
        !enabled()}
      onclick={async () => {
        if (await session.accept()) onaccepted();
      }}
      >Apply {result?.changed ?? 0} change{result?.changed === 1
        ? ''
        : 's'}</button
    >
    <small>One Undo step · Preview only until applied</small>
  </footer>
</section>

<style>
  .timing-review {
    container-type: inline-size;
    height: 100%;
    display: flex;
    flex-direction: column;
    font-size: var(--font-size-sm);
    min-height: 0;
  }
  .scroll {
    overflow: auto;
    flex: 1;
    min-height: 0;
  }
  .rules,
  .policy,
  footer,
  .heading,
  .manual,
  .change {
    padding: 6px 10px;
    border-top: 1px solid var(--line);
  }
  .rules,
  .policy {
    background: var(--surface-raised);
  }
  .rules label,
  .heading,
  nav {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .rules label {
    padding: 3px 0;
  }
  .rules .number,
  .heading button:not(:first-child) {
    margin-left: auto;
  }
  .number,
  .cue {
    font-family: var(--font-mono);
    font-size: inherit;
    font-variant-numeric: tabular-nums;
  }
  .policy {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 6px;
    align-items: center;
  }
  .policy small {
    grid-column: 1/-1;
  }
  .policy input[type='number'] {
    width: 6em;
  }
  .switch {
    display: flex;
    flex-wrap: wrap;
  }
  .switch label {
    position: relative;
    border: 1px solid var(--line-strong);
    padding: var(--control-pad-y) var(--control-pad-x);
    cursor: pointer;
  }
  .switch label:has(:checked) {
    color: var(--accent-text);
    background: var(--accent-soft);
  }
  .switch input {
    opacity: 0;
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    cursor: pointer;
  }
  .switch label:focus-within {
    outline: 2px solid var(--focus);
  }
  .change {
    display: grid;
    grid-template-columns: auto minmax(3ch, auto) minmax(0, 1fr);
    column-gap: 6px;
    align-items: baseline;
  }
  .change button {
    white-space: normal;
    text-align: left;
    padding: 0;
  }
  .cue {
    color: var(--accent-text);
  }
  .effect {
    grid-column: 3;
  }
  .times {
    grid-column: 3/-1;
  }
  .times span,
  small {
    display: block;
  }
  .chip {
    background: var(--surface-hover);
    color: var(--text);
    border-radius: var(--radius-sm);
    padding: 1px 4px;
    margin-right: 4px;
  }
  .moved {
    color: var(--accent-text);
    background: var(--accent-soft);
  }
  .trade,
  .warning {
    color: var(--warn);
  }
  .trade {
    background: var(--warn-soft);
  }
  .manual {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 8px;
    align-items: baseline;
  }
  .heading button:first-child {
    padding: 0;
  }
  footer button {
    width: 100%;
  }
  footer small,
  p,
  small {
    color: var(--text-muted);
  }
  p {
    margin: 4px 0;
  }
  footer small {
    margin-top: 4px;
  }
  nav {
    padding: 6px;
  }
  [role='alert'] {
    color: var(--err);
  }
  @container (max-width:400px) {
    .policy {
      grid-template-columns: minmax(0, 1fr);
    }
    .switch {
      width: 100%;
    }
  }
</style>
