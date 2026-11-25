<script lang="ts">
  import { settled } from '../ui/settled.svelte';
  import { onMount, tick, untrack } from 'svelte';
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
  import type { WorkspaceActivity } from '../workspace/workspace-activity.svelte';
  let {
    activity,
    view,
    subtitles,
    mediaSession,
    scopeIds,
    previewIds,
    context,
    enabled,
    options,
    previewEnabled,
    onoptions,
    onbusy,
    onpreview,
    onaccepted,
    onnavigate,
  }: {
    activity: WorkspaceActivity;
    view: WorkspaceView;
    subtitles: DocumentSession;
    mediaSession: () => number;
    scopeIds: () => number[] | null;
    previewIds: () => readonly number[];
    context: () => string;
    enabled: () => boolean;
    options: TimingFixOptions;
    previewEnabled: () => boolean;
    onoptions: (o: TimingFixOptions) => void;
    onbusy: (v: boolean) => void;
    onpreview: (v: TimingPreview | null) => void;
    onaccepted: () => void;
    onnavigate: (id: string, origin: HTMLButtonElement) => void;
  } = $props();
  const reviewId = $props.id();
  const labels: Record<keyof TimingFamilies, string> = {
    gaps: 'Gaps and overlaps',
    speed: 'Duration and reading speed',
    shots: 'Shot changes',
  };
  const familyKeys = ['gaps', 'speed', 'shots'] as const;
  const choices = $derived(view.timingSelection);
  let manualPage = $state.raw<TimingFixView | null>(null);
  let manualError = $state('');
  const input = (): TimingFixInput => ({
    options: { ...options },
    families: { ...view.families },
    scope: scopeIds(),
    toggled: choices.toggled,
    omitTrades: false,
  });
  const session = untrack(
    () =>
      new TimingFixSession(subtitles, mediaSession, input, {
        activity,
        retainDisplay: true,
        enabled,
        previewEnabled,
        selection: () => null,
        pageKey: () => choices.selectionKey,
        initialOffset: () => choices.changesOffset,
        context,
        previewIds,
      }),
  );
  const current = $derived(
    session.current && session.view?.phase === 'reviewing'
      ? session.view
      : null,
  );
  const documentKey = $derived(
    JSON.stringify([subtitles.snapshot.generation, mediaSession(), context()]),
  );
  let scroll: HTMLDivElement;
  function rows() {
    return Array.from(scroll.querySelectorAll<HTMLElement>('[data-anchor]'));
  }
  function keepScroll() {
    if (!scroll || !current) return;
    const top = scroll.getBoundingClientRect().top;
    const list = rows();
    const row = list.find((el) => el.getBoundingClientRect().bottom > top);
    choices.anchor =
      row && scroll.scrollTop > 0
        ? {
            key: row.dataset.anchor!,
            index: Number(row.dataset.index),
            offset: row.getBoundingClientRect().top - top,
          }
        : null;
  }
  function restoreAnchor() {
    const anchor = choices.anchor;
    if (!anchor || !scroll) return;
    const list = rows();
    const row =
      list.find((el) => el.dataset.anchor === anchor.key) ??
      list.reduce<HTMLElement | undefined>(
        (nearest, el) =>
          !nearest ||
          Math.abs(Number(el.dataset.index) - anchor.index) <
            Math.abs(Number(nearest.dataset.index) - anchor.index)
            ? el
            : nearest,
        undefined,
      );
    if (row)
      scroll.scrollTop +=
        row.getBoundingClientRect().top -
        scroll.getBoundingClientRect().top -
        anchor.offset;
  }
  const pageId = $derived(current?.id ?? null);
  const changesOffset = $derived(current?.offset ?? null);
  // Polls return a fresh view of the same immutable proposal. Restoring on
  // every reply can replay the old anchor before a user's scroll event arrives.
  $effect(() => {
    if (pageId !== null && changesOffset !== null) {
      choices.changesOffset = changesOffset;
      void tick().then(restoreAnchor);
    }
  });
  const result = $derived(current ?? session.displayed?.view);
  const updating = $derived(!current || session.accepting);
  const visual = settled(() => updating || !session.preview);
  const coverage = $derived(result?.coverage);
  const blocked = $derived(
    coverage === 'partial' || coverage === 'unavailable',
  );
  const unavailable = $derived(coverage === 'noMedia' || blocked);
  $effect(() => {
    const id = pageId,
      offset = choices.manualOffset,
      key = documentKey;
    let current = true;
    if (manualKey !== key) manualPage = null;
    manualKey = key;
    manualError = '';
    if (id !== null)
      void timingFixView(id, offset, 100, 'manual').then(
        (v) => {
          if (current) {
            manualPage = v;
            void tick().then(restoreAnchor);
          }
        },
        (e) => {
          if (current) manualError = String(e);
        },
      );
    return () => {
      current = false;
    };
  });
  let manualKey = '';

  function toggle(index: number) {
    keepScroll();
    choices.toggled = [...choices.toggled, index];
  }
  function untickTrades() {
    keepScroll();
    choices.toggled = [
      ...choices.toggled,
      ...(result?.withoutTradesToggles ?? []),
    ];
  }
  function setPolicy(patch: Partial<TimingFixOptions>) {
    onoptions({ ...options, ...patch });
  }
  $effect(() => {
    const busy = !session.canClose;
    untrack(() => onbusy(busy));
  });
  $effect(() => {
    const last = session.displayed;
    const preview = last?.preview
      ? {
          document: last.document,
          retained: { session: last.session, context: last.context },
          ghosts: timingGhosts(last.preview).map((ghost) => ({
            ...ghost,
            updating: visual.showUpdating,
          })),
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
  <div class="scroll" bind:this={scroll} onscroll={keepScroll}>
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
    {#if result}
      <div class="results" aria-busy={visual.showUpdating}>
        {#if result.withheldAll}<p role="status">
            Timing changes were withheld because the safety check could not
            finish. Try a smaller scope.
          </p>{/if}
        <div class="heading">
          <strong class="number"
            >{result.before.total} → {result.after.total}</strong
          > findings
        </div>
        <button
          disabled={updating ||
            (!result.selectionError && !choices.toggled.length)}
          onclick={() => {
            keepScroll();
            choices.toggled = [];
          }}>Restore default selection</button
        >
        <div class="heading">
          <strong>Changes</strong><span class="number">{result.total}</span
          >{#if result.trades}<button
              data-variant="quiet"
              disabled={updating}
              onclick={untickTrades}>Untick trade-offs ({result.trades})</button
            >{/if}
        </div>
        {#if !result.total}<p>
            No changes for the ticked rules in this scope.
          </p>{/if}
        {#each result.rows as row (row.cueIndex)}
          {@const expanded = choices.openRows.includes(row.cueIndex)}
          {@const timecodesId = reviewId + '-timecodes-' + row.cueId}
          <div
            class="change"
            data-anchor={'change-' + row.cueId}
            data-index={row.cueIndex}
          >
            <button
              data-nav-key={'change-' + row.cueId}
              class="cue"
              data-variant="quiet"
              aria-label={'Go to cue ' + row.cueLabel}
              onclick={(event) => onnavigate(row.cueId, event.currentTarget)}
              >{row.cueLabel}</button
            >
            <input
              type="checkbox"
              aria-label={'Include cue ' + row.cueLabel}
              checked={row.selected}
              disabled={updating}
              onchange={() => toggle(row.cueIndex)}
            />
            <button
              class="detail"
              data-variant="quiet"
              aria-label={'Timecodes for cue ' + row.cueLabel}
              aria-expanded={expanded}
              aria-controls={timecodesId}
              onclick={() =>
                (choices.openRows = expanded
                  ? choices.openRows.filter((i) => i !== row.cueIndex)
                  : [...choices.openRows, row.cueIndex])}
            >
              {expanded ? '▾' : '▸'}
            </button>
            <div class="summary">
              {#each [...new Set(row.unit.fixed.map(chip))] as name (name)}<span
                  class="chip">{name}</span
                >{/each}
              {#if row.unit.partial}<span class="chip">Partial</span>{/if}
              {#if row.startMoved}<span class="chip moved">Start moved</span
                >{/if}
              {#if row.unit.tradeOff}<span class="chip trade">Trade-off</span
                >{/if}<span class="number">{row.delta}</span>{#if row.needs}
                <small>{row.needs}</small>{/if}
              {#if row.replaces}<small>{row.replaces}</small>{/if}
              {#if row.offReason}<small>{row.offReason}</small>{/if}
            </div>
            <!-- eslint-disable svelte/no-useless-mustaches -- A literal interpolation preserves both spaces across Svelte whitespace trimming. -->
            <small class="effect number"
              >{#each row.unit.effectsText as effect, index (index)}{#if index}{' · '}{/if}<span
                  class:warning={effect.warning}>{effect.text}</span
                >{/each}</small
            >
            <!-- eslint-enable svelte/no-useless-mustaches -->
            <small id={timecodesId} hidden={!expanded} class="times number"
              >{#each row.timecodes as line (line)}<span>{line}</span
                >{/each}</small
            >
          </div>
        {/each}
        {#if result.total > 100}<nav aria-label="Timing proposal pages">
            <button
              disabled={updating || !result.offset}
              onclick={() => session.page(Math.max(0, result!.offset - 100))}
              >Previous</button
            >{result.offset + 1}–{result.offset + result.rows.length} / {result.total}<button
              disabled={updating || result.offset + 100 >= result.total}
              onclick={() => session.page(result!.offset + 100)}>Next</button
            >
          </nav>{/if}
        {#if result.manualCount}
          <div class="heading">
            <button
              data-variant="quiet"
              aria-expanded={choices.manualOpen}
              onclick={() => (choices.manualOpen = !choices.manualOpen)}
              >{choices.manualOpen ? '▾' : '▸'} Needs manual work {result.manualCount}</button
            >
          </div>
          {#if choices.manualOpen || subtitles.dirty}
            {#each manualPage?.manual ?? [] as r (r.cueId)}<div
                class="manual"
                data-anchor={'manual-' + r.cueId}
                data-index={r.cueNumber - 1}
              >
                <button
                  class="cue"
                  data-variant="quiet"
                  data-nav-key={'manual-cue-' + r.cueId}
                  aria-label={'Go to manual cue ' + r.cueNumber}
                  onclick={(event) => onnavigate(r.cueId, event.currentTarget)}
                  >{r.cueNumber}</button
                >
                <div>
                  <span>{r.detail.lead}</span>
                  {#each r.detail.facts as fact (fact)}<small>{fact}</small
                    >{/each}
                  {#if r.detail.budgetText}<small class="budget"
                      >{r.detail.budgetText}</small
                    >{/if}
                </div>
                <button
                  data-nav-key={'manual-' + r.cueId}
                  onclick={(event) => onnavigate(r.cueId, event.currentTarget)}
                  >Edit</button
                >
              </div>{/each}
            {#if manualPage && result.manualCount > 100}<nav
                aria-label="Manual work pages"
              >
                <button
                  disabled={updating || !manualPage.offset}
                  onclick={() =>
                    (choices.manualOffset = Math.max(
                      0,
                      manualPage!.offset - 100,
                    ))}>Previous</button
                >{manualPage.offset + 1}–{manualPage.offset +
                  manualPage.manual.length} / {result.manualCount}<button
                  disabled={updating ||
                    manualPage.offset + 100 >= result.manualCount}
                  onclick={() =>
                    (choices.manualOffset = manualPage!.offset + 100)}
                  >Next</button
                >
              </nav>{/if}
          {/if}
        {:else if !result.total}<p>
            No timing findings for the ticked rules in this scope.
          </p>{/if}
      </div>
    {/if}
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
    <small>One Undo step · Preview only until applied</small>
  </div>
  <footer>
    <div class="preview-status">
      {#if session.error || manualError || session.view?.phase === 'failed' || session.view?.phase === 'stale'}
        <span
          role="alert"
          title={session.error ||
            manualError ||
            session.view?.error ||
            'Preview is stale. Check the preview again.'}
          >{session.error ||
            manualError ||
            session.view?.error ||
            'Preview is stale. Check the preview again.'}</span
        >
        <button onclick={() => session.retry()}>Retry preview</button>
      {:else if current?.selectionError}<span
          role="alert"
          title={current.selectionError}>{current.selectionError}</span
        >
      {:else}
        <span role="status"
          >{subtitles.dirty
            ? 'Waiting for the cue edit to commit.'
            : visual.showUpdating
              ? session.accepting
                ? 'Applying timings…'
                : 'Updating preview…'
              : ''}</span
        >
      {/if}
    </div>
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
  .results[aria-busy='true'] {
    opacity: 0.55;
  }
  .preview-status {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    overflow: hidden;
  }
  .preview-status span {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .preview-status button {
    width: auto;
    flex: none;
  }
  .scroll {
    overflow-anchor: none;
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
  .change,
  .manual {
    isolation: isolate;
    position: relative;
  }
  .cue::after {
    content: '';
    position: absolute;
    inset: 0;
  }
  .change input,
  .detail,
  .manual button:not(.cue) {
    position: relative;
    z-index: 1;
  }
  .change {
    display: grid;
    grid-template-columns: minmax(3ch, auto) auto auto minmax(0, 1fr);
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
  .change .detail {
    min-width: 24px;
    text-align: center;
  }
  .effect,
  .times {
    grid-column: 4;
  }
  .times[hidden] {
    display: none;
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
  footer {
    flex: none;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
  }
  .preview-status {
    flex: 1;
    min-width: 0;
  }
  footer .apply {
    flex: none;
  }

  p,
  small {
    color: var(--text-muted);
  }
  p {
    margin: 4px 0;
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
