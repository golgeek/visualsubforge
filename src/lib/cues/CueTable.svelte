<script lang="ts">
  import { countLabel } from '../ui/count-label';
  import { timingDelta, type TimingGhost } from '../timing/review-preview';
  import { settled } from '../ui/settled.svelte';
  import { onMount, tick, untrack, type Snippet } from 'svelte';
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { QualitySession } from '../quality/quality-session.svelte';
  import type { Workspace } from '../workspace/workspace.svelte';
  import type { WorkspaceLayout } from '../layout/workspace-layout.svelte';
  import DiffText from '../review/DiffText.svelte';
  import type { ReviewSession } from '../review/review-session.svelte';
  import {
    checkKey,
    cueWords,
    signedDelta,
    type TableFilter,
  } from '../review/presentation';
  import type { ReviewChange } from '../client';
  import type { QualityMetric } from '../client';
  import { formatTime } from '../timeline/timeline';
  import { rate } from '../quality/quality-display';
  import Chip from '../ui/Chip.svelte';
  import Meter from '../ui/Meter.svelte';
  import {
    columnGrid,
    columnLabels,
    resizeColumn,
    type ColumnId,
    type CueRow,
  } from './cue-columns';
  import type { SelectionMode } from './cue-selection';
  import type { CueTableView } from './cue-table-view.svelte';
  let {
    review,
    ghosts = [],
    subtitles,
    quality,
    tableView,
    workspace,
    layout,
    list = $bindable(),
    actions,
    controls,
  }: {
    review: ReviewSession;
    ghosts?: TimingGhost[];
    subtitles: DocumentSession;
    quality: QualitySession;
    tableView: CueTableView;
    workspace: Workspace;
    layout: WorkspaceLayout;
    list: HTMLDivElement | undefined;
    controls?: Snippet;
    actions: {
      listScrolled: (top: number) => void;
      columns: () => void;
      openCueMenu: (
        x: number,
        y: number,
        id: number | null,
        row: HTMLButtonElement,
      ) => Promise<void>;
      selectCue: (id: number, mode?: SelectionMode) => Promise<void>;
      cueSelectionKey: (event: KeyboardEvent, id: number) => void;
      focusText: () => Promise<void>;
    };
  } = $props();
  const changes = $derived(
    new Map(
      review.projection.flatMap((c) =>
        c.revised.map((cue) => [cue.cue_id, c] as const),
      ),
    ),
  );
  const glyph = {
    text: 'T',
    timing: '◷',
    text_and_timing: '±',
    added: '+',
    removed: '−',
    split: '⑂',
    merged: '⑃',
    style: 'S',
    unchanged: '',
  };
  const filters: [TableFilter, string][] = [
    ['all', 'All cues'],
    ['changed', 'Changed only'],
    ['threads', 'With threads'],
  ];
  function oldTime(
    c: ReviewChange | undefined,
    id: 'start' | 'end',
    current: number,
  ) {
    if (!c?.timing_changed || !c.original.length) return null;
    const old =
      id === 'start'
        ? Math.min(...c.original.map((q) => q.start_ms))
        : Math.max(...c.original.map((q) => q.end_ms));
    return old === current ? null : { old, delta: signedDelta(current - old) };
  }
  const deltas = $derived(new Map(ghosts.map((ghost) => [ghost.id, ghost])));
  const doc = $derived(subtitles.snapshot);
  const columns = $derived(tableView.visibleColumns);
  const checkWidth = $derived.by(() => {
    let count = 1;
    for (const rows of review.index.removals.values())
      count = Math.max(count, rows.length + 1);
    return 24 * Math.min(count, 4);
  });
  const minimumWidth = $derived(
    columns.reduce(
      (sum, c) => sum + c.width,
      (review.attached ? 50 + checkWidth : 20) +
        Math.max(0, columns.length - 1) * 6,
    ),
  );
  const activeIds = $derived(tableView.activeIds);
  let horizontal = $state(0);
  let drag: {
    id: ColumnId;
    pointer: number;
    start: number;
    width: number;
    preset: string;
  } | null = null;
  function cell(
    row: CueRow,
    id: ColumnId,
    metric: QualityMetric | undefined,
  ): string {
    switch (id) {
      case 'number':
        return String(row.index + 1);
      case 'start':
        return formatTime(row.cue.startMs);
      case 'end':
        return formatTime(row.cue.endMs);
      case 'duration':
        return metric?.data.durationMs == null
          ? '—'
          : `${(metric.data.durationMs / 1000).toFixed(3)} s`;
      case 'cps':
        return metric ? rate(metric.data.cps) : '—';
      case 'rs':
        return metric ? rate(metric.data.rs) : '—';
      case 'longestLine':
        return metric ? String(metric.longestLine) : '—';
      case 'lineCount':
        return metric ? String(metric.lineCount) : '—';
      case 'gap':
        return metric?.data.gapMs == null ? '—' : `${metric.data.gapMs} ms`;
      case 'style':
        return row.cue.style ?? '—';
      case 'actor':
        return row.cue.actor || '—';
      case 'text':
        return row.cue.text.replaceAll('\n', ' ↵ ') || '(empty)';
      default:
        return '';
    }
  }
  function startResize(event: PointerEvent, id: ColumnId) {
    if (event.button !== 0 || !workspace.canAct) return;
    drag = {
      id,
      pointer: event.pointerId,
      start: event.clientX,
      width: (
        event.currentTarget as HTMLElement
      ).parentElement!.getBoundingClientRect().width,
      preset: layout.preset,
    };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    event.preventDefault();
  }
  function moveResize(event: PointerEvent) {
    if (
      !drag ||
      drag.pointer !== event.pointerId ||
      drag.preset !== layout.preset
    )
      return;
    layout.setColumns(
      resizeColumn(
        layout.columns,
        drag.id,
        drag.width + event.clientX - drag.start,
      ),
    );
  }
  function fit(id: ColumnId) {
    if (!list) return;
    const cells = [
      ...list.querySelectorAll<HTMLElement>(`[data-cell="${id}"]`),
    ];
    const textWidth = (element: Element) => {
      const range = document.createRange();
      range.selectNodeContents(element);
      return range.getBoundingClientRect().width;
    };
    const header = list.parentElement?.querySelector(
      `.column-heading[data-column="${id}"] > span`,
    );
    const width = Math.max(
      header ? textWidth(header) + 12 : 40,
      ...cells.map(
        (cell) => textWidth(id === 'cps' ? cell.firstElementChild! : cell) + 12,
      ),
    );
    layout.setColumns(resizeColumn(layout.columns, id, width));
  }
  const updating = settled(
    () => quality.refreshStarted && quality.retainingIndex,
  );
  const resetKey = $derived(
    JSON.stringify([doc.generation, tableView.filterKey, layout.preset]),
  );
  let oldKey = '';
  let oldRows: CueRow[] = [];
  $effect.pre(() => {
    const key = resetKey;
    const rows = tableView.rows;
    const previous = oldRows;
    const reset = oldKey !== key;
    oldRows = rows;
    oldKey = key;
    if (!list) return;
    const element = list;
    const height = tableView.rowHeight;
    const top = element.scrollTop;
    const first = Math.floor(top / height);
    if (
      !reset &&
      (!top ||
        !previous[first] ||
        previous[first].cue.id === rows[first]?.cue.id)
    )
      return;
    let target = 0;
    if (!reset) {
      // Prefer the next surviving neighbour when two are equally near.
      let index = -1;
      for (
        let distance = 0;
        distance < previous.length && index < 0;
        distance++
      ) {
        for (const pos of [first + distance, first - distance]) {
          const cue = previous[pos]?.cue;
          if (cue && (index = untrack(() => tableView.indexOf(cue.id))) >= 0)
            break;
        }
      }
      target = Math.max(0, index * height + (top % height));
    }
    let current = true;
    void tick().then(() => {
      if (!current || !element.isConnected) return;
      element.scrollTop = target;
      tableView.scrollTop = element.scrollTop;
    });
    return () => {
      current = false;
    };
  });
  onMount(() => {
    const observer = new ResizeObserver(([entry]) => {
      if (entry) tableView.height = entry.contentRect.height;
    });
    if (list) observer.observe(list);
    return () => {
      drag = null;
      observer.disconnect();
    };
  });
</script>

{#snippet removalMarker(rows: import('../review/presentation').ReviewRow[])}
  {#if rows.length}<b
      class="removed-tag"
      title={rows
        .map(
          (r) =>
            'Removed before this cue: #' +
            (r.sourceIndex + 1) +
            ' ' +
            (review.removalDetails
              .get(checkKey(r))
              ?.original.map((q) => q.text)
              .join(' / ') ?? ''),
        )
        .join('\n')}>−{rows.length}</b
    >{/if}
{/snippet}
<div
  class="cue-table"
  style:--check-width={`${checkWidth}px`}
  style:--column-grid={`${doc.review?.path ? '22px ' : ''}${columnGrid(columns)}${review.attached ? ' var(--check-width)' : ''}`}
  style:--columns-min={`${minimumWidth}px`}
  style:--row-height={`${tableView.rowHeight}px`}
>
  <div class="list-controls">
    <button disabled={!workspace.canAct} onclick={actions.columns}
      >Columns ▾</button
    >
    <input
      autocomplete="off"
      autocorrect="off"
      autocapitalize="off"
      spellcheck="false"
      class="list-filter"
      aria-label="Filter cues by text or number"
      placeholder="Text or #"
      bind:value={tableView.query}
    />
    <Chip
      active={tableView.issuesOnly}
      onclick={() => (tableView.issuesOnly = !tableView.issuesOnly)}
      >{countLabel(
        quality.issueIndexComplete || quality.retainingIndex
          ? quality.displayedIssues.size
          : '…',
        'cue',
      )} with issues</Chip
    >
    <span class="cue-count"
      >{tableView.rows.length} / {doc.cues.length} · {subtitles.selectedIds
        .size} selected</span
    >
    {@render controls?.()}
  </div>
  {#if review.attached}
    <div class="review-controls">
      <div class="segmented" role="group" aria-label="Review cues">
        {#each filters as [value, label] (value)}<button
            aria-pressed={review.tableFilter === value}
            onclick={() => review.setTableFilter(value)}
            >{label}
            <small
              >{value === 'all'
                ? doc.cues.length
                : review.summary
                  ? value === 'changed'
                    ? review.index.changed.size
                    : review.index.threaded.size
                  : '…'}</small
            ></button
          >{/each}
      </div>
      <div class="stepper">
        <button
          aria-label="Previous table change"
          onclick={() => void review.navigate(false)}>‹</button
        >
        <span>Stop {review.position || '—'} of {review.stopCount}</span>
        <button
          aria-label="Next table change"
          onclick={() => void review.navigate(true)}>›</button
        >
      </div>
      <span>{review.checked.size}/{review.progress.total} checked</span>
    </div>
  {/if}
  <div class="header-clip">
    <div
      class="table-head"
      role="toolbar"
      tabindex="-1"
      aria-label="Cue column headers"
      style:transform={`translateX(${-horizontal}px)`}
      oncontextmenu={(event) => {
        event.preventDefault();
        if (workspace.canAct) actions.columns();
      }}
    >
      {#if doc.review?.path}<span aria-label="Changes">Δ</span>{/if}
      {#each columns as column (column.id)}
        <div class="column-heading" data-column={column.id}>
          <span>{column.id === 'issue' ? '!' : columnLabels[column.id]}</span>
          {#if column.id !== 'issue'}<button
              class="resize-edge"
              aria-label={`Resize ${columnLabels[column.id]} column`}
              onpointerdown={(e) => startResize(e, column.id)}
              onpointermove={moveResize}
              onpointerup={() => (drag = null)}
              onpointercancel={() => (drag = null)}
              onlostpointercapture={() => (drag = null)}
              ondblclick={() => fit(column.id)}
              onkeydown={(e) => {
                if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
                  e.preventDefault();
                  layout.setColumns(
                    resizeColumn(
                      layout.columns,
                      column.id,
                      column.width + (e.key === 'ArrowRight' ? 8 : -8),
                    ),
                  );
                } else if (e.key === 'Home') {
                  e.preventDefault();
                  fit(column.id);
                }
              }}
            ></button>{/if}
        </div>
      {/each}
      {#if review.attached}<span aria-label="Checked">✓</span>{/if}
    </div>
  </div>
  <div
    class="cue-list"
    bind:this={list}
    onscroll={(event) => {
      horizontal = event.currentTarget.scrollLeft;
      actions.listScrolled(event.currentTarget.scrollTop);
    }}
  >
    {#if !doc.loaded}<p class="empty">
        Create a cue to start new subtitles, or open SRT, ASS, SSA or WebVTT.
      </p>
    {:else if doc.cues.length === 0}<p class="empty">
        No cues yet. Create a cue to start editing.
      </p>
    {:else if tableView.rows.length === 0}<p class="empty">
        {tableView.issuesOnly && !quality.issueIndexComplete
          ? 'Waiting for issue results…'
          : 'No cues match this filter.'}
      </p>
    {:else}<div
        style:height={`${tableView.rows.length * tableView.rowHeight}px`}
        class="virtual-list"
      >
        <div
          style:transform={`translateY(${tableView.firstRow * tableView.rowHeight}px)`}
        >
          {#each tableView.visibleRows as row (row.cue.id)}
            {@const item = row.cue}
            {@const metric = quality.displayedMetrics.get(String(item.id))}
            {@const issue = quality.displayedIssues.get(String(item.id))}
            {@const change = changes.get(item.id)}
            {@const summary = review.index.byCue.get(item.id)}
            {@const kind = summary?.kind ?? change?.kind ?? 'unchanged'}
            {@const removed = review.index.removals.get(item.id) ?? []}
            {@const part =
              change?.revised.findIndex((q) => q.cue_id === item.id) ?? 0}
            <div class="row-wrap">
              <button
                class="cue-row"
                class:resolved={tableView.resolvedCue === item.id}
                data-kind={kind}
                class:removed-before={removed.length > 0}
                class:selected={subtitles.selectedIds.has(item.id)}
                class:active={activeIds.includes(item.id)}
                aria-current={activeIds.includes(item.id) ? 'time' : undefined}
                aria-pressed={subtitles.selectedIds.has(item.id)}
                data-cue-id={item.id}
                aria-disabled={workspace.busy}
                oncontextmenu={(e) => {
                  e.preventDefault();
                  void actions.openCueMenu(
                    e.clientX,
                    e.clientY,
                    item.id,
                    e.currentTarget,
                  );
                }}
                onclick={(e) =>
                  void actions.selectCue(
                    item.id,
                    e.shiftKey
                      ? e.ctrlKey || e.metaKey
                        ? 'addRange'
                        : 'range'
                      : e.ctrlKey || e.metaKey
                        ? 'toggle'
                        : 'replace',
                  )}
                onkeydown={(e) => {
                  if (
                    e.key === 'ContextMenu' ||
                    (e.key === 'F10' && e.shiftKey)
                  ) {
                    e.preventDefault();
                    e.stopPropagation();
                    const row = e.currentTarget;
                    const box = row.getBoundingClientRect();
                    void actions.openCueMenu(
                      box.left,
                      box.bottom,
                      item.id,
                      row,
                    );
                  } else actions.cueSelectionKey(e, item.id);
                }}
                ondblclick={() => void actions.focusText()}
              >
                {#if doc.review?.path}<span
                    class="change-marker"
                    class:bracket={summary?.block != null}
                    aria-label={kind === 'unchanged'
                      ? 'Unchanged'
                      : `Change: ${kind}`}
                    title={summary?.shiftMs != null
                      ? `Shift block: ${signedDelta(summary.shiftMs)}`
                      : kind}
                    >{glyph[
                      kind
                    ]}{#if summary?.hasThreads || (change && review.threads(change, true).length)}
                      <i
                        class="thread-dot"
                        class:resolved={summary && !summary.open}
                        aria-label="Commented cue"
                        title={summary?.open
                          ? 'Open thread'
                          : 'Resolved thread'}
                      ></i>
                    {/if}</span
                  >{/if}
                {#each columns as column (column.id)}
                  {#if column.id === 'issue'}<span
                      class="issue"
                      aria-busy={updating.showUpdating &&
                        quality.checkingCue(item.id)}
                      class:violation={issue === 'violation'}
                      class:advisory={issue === 'advisory'}
                      aria-label={tableView.resolvedCue === item.id
                        ? 'Resolved'
                        : issue === 'violation'
                          ? 'Error'
                          : issue === 'advisory'
                            ? 'Warning'
                            : quality.issueIndexComplete ||
                                quality.retainingIndex
                              ? 'No reported issue'
                              : 'Issues not yet available'}
                      >{tableView.resolvedCue === item.id
                        ? '✓'
                        : issue === 'violation'
                          ? '!'
                          : issue === 'advisory'
                            ? '•'
                            : quality.issueIndexComplete ||
                                quality.retainingIndex
                              ? ''
                              : '…'}</span
                    >
                  {:else if column.id === 'original'}<span
                      data-cell="original"
                      class:bracket={kind === 'split' || kind === 'merged'}
                    >
                      {@render removalMarker(removed)}
                      {#if kind === 'added'}<b class="added-chip">Added</b>
                      {:else if change && part > 0}<small
                          >↳ split from #{(change.original[0]?.source_index ??
                            0) + 1}</small
                        >
                      {:else if change}<DiffText
                          words={change.words}
                          side="original"
                          singleLine
                        />
                      {:else if kind === 'unchanged'}{item.text.replaceAll(
                          '\n',
                          ' ↵ ',
                        )}{/if}
                    </span>
                  {:else if column.id === 'start' || column.id === 'end'}
                    {@const old = oldTime(
                      change,
                      column.id,
                      column.id === 'start' ? item.startMs : item.endMs,
                    )}
                    <span
                      data-cell={column.id}
                      class:tinted={old}
                      title={old
                        ? `Was ${formatTime(old.old)} · ${old.delta}`
                        : undefined}
                    >
                      {cell(row, column.id, metric)}
                      {#if old && tableView.columns.density === 'comfortable'}<small
                          class="prior"
                          >{formatTime(old.old)} · {old.delta}</small
                        >{/if}
                    </span>
                  {:else if column.id === 'cps'}<span
                      class="cps"
                      data-cell="cps"
                      ><span>{cell(row, column.id, metric)}</span><Meter
                        label="CPS"
                        value={metric?.data.cps.status === 'finite'
                          ? metric.data.cps.value
                          : null}
                        maximum={Math.max(
                          40,
                          metric?.data.cps.status === 'finite'
                            ? (metric.data.cps.value ?? 0)
                            : 0,
                        )}
                      /></span
                    >
                  {:else}<span
                      class:cue-text={column.id === 'text'}
                      data-cell={column.id}
                      >{#if column.id === 'text' && !columns.some((c) => c.id === 'original')}{@render removalMarker(
                          removed,
                        )}{/if}{#if column.id === 'text' && change && review.attached}<DiffText
                          words={cueWords(change, 'revised', part)}
                          side="revised"
                          singleLine
                        />{:else}{cell(
                          row,
                          column.id,
                          metric,
                        )}{/if}{#if column.id === 'text' && deltas.has(row.cue.id)}<small
                          class="timing-delta"
                          class:updating={deltas.get(row.cue.id)?.updating}
                          data-preview-cue={row.cue.id}
                          >{timingDelta(deltas.get(row.cue.id)!)}</small
                        >{/if}</span
                    >{/if}
                {/each}
              </button>
              <div class="checks">
                {#each removed as removal (checkKey(removal))}
                  <input
                    type="checkbox"
                    aria-label={`Checked removed cue ${removal.sourceIndex + 1}`}
                    title={`Removed · was #${removal.sourceIndex + 1}`}
                    checked={review.checked.has(checkKey(removal))}
                    onchange={() => review.toggleCheck(removal)}
                  />
                {/each}
                {#if summary && kind !== 'unchanged' && summary.revised[0] === item.id && (summary.block == null || summary.index === summary.block)}
                  <input
                    type="checkbox"
                    aria-label={`Checked cue ${row.index + 1}`}
                    checked={summary.block == null
                      ? review.checked.has(checkKey(summary))
                      : review.index.blocks
                          .get(summary.block)
                          ?.every((r) => review.checked.has(checkKey(r)))}
                    onchange={() => review.toggleCheck(summary, true)}
                  />
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </div>{/if}
  </div>
</div>

<style>
  .review-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 3px 8px;
    background: var(--surface-panel);
    font-size: 11px;
  }
  .segmented,
  .stepper {
    display: inline-flex;
    align-items: center;
  }
  .segmented {
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .segmented button,
  .stepper button {
    padding: 1px 6px;
  }
  .segmented button {
    border: 0;
    border-radius: 0;
    background: transparent;
  }
  .segmented button + button {
    border-left: 1px solid var(--line-strong);
  }
  .segmented [aria-pressed='true'] {
    background: var(--selection);
    box-shadow: inset 0 -2px var(--accent);
  }
  .stepper {
    gap: 4px;
    font-family: var(--font-mono);
  }
  .row-wrap {
    position: relative;
  }
  .checks {
    display: flex;
    gap: 3px;
    align-items: center;
    max-width: var(--check-width);
    overflow-x: auto;
    position: absolute;
    right: 12px;
    top: 50%;
    transform: translateY(-50%);
    margin: 0;
  }
  .checks input {
    margin: 0;
    flex: none;
  }
  .removed-before {
    border-top: 2px solid var(--kind-removed);
  }
  .removed-tag,
  .added-chip {
    border-radius: 4px;
    padding: 0 4px;
    margin-right: 3px;
    font-size: 10px;
  }
  .removed-tag {
    background: var(--kind-removed-soft);
    color: var(--kind-removed);
  }
  .added-chip {
    background: var(--kind-added-soft);
    color: var(--kind-added);
  }
  .cue-row[data-kind='added'] {
    box-shadow: inset 3px 0 var(--kind-added);
  }
  .thread-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--thread-open);
  }
  .thread-dot.resolved {
    background: transparent;
    border: 1px solid var(--thread-resolved);
  }
  [data-cell='original'] {
    color: var(--text-muted);
  }
  .bracket {
    border-inline-start: 2px solid var(--kind-group);
    padding-left: 3px;
  }
  .change-marker.bracket {
    border-color: var(--kind-timing);
  }
  .change-marker {
    color: var(--kind-text);
  }
  [data-kind='timing'] .change-marker,
  .tinted {
    color: var(--kind-timing);
  }
  [data-kind='added'] .change-marker {
    color: var(--kind-added);
  }
  [data-kind='split'] .change-marker,
  [data-kind='merged'] .change-marker {
    color: var(--kind-group);
  }
  [data-kind='style'] .change-marker {
    color: var(--kind-style);
  }
  .tinted {
    background: var(--kind-timing-soft);
  }
  .prior {
    display: block;
    font-size: 9px;
    line-height: 1;
  }

  .timing-delta.updating {
    opacity: 0.55;
  }
  .timing-delta {
    margin-left: 8px;
    color: var(--accent);
    border: 1px dashed var(--accent);
    padding: 0 3px;
    font-size: 10px;
  }
  .cue-row.resolved,
  .issue[aria-busy='true'] {
    opacity: 0.65;
  }
  .cue-table {
    container-type: inline-size;
    container-name: cue-table;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 64px;
  }
  .list-controls {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 30px;
    padding: 2px 8px;
    background: var(--surface-panel);
    font-size: 11px;
  }
  .list-controls button {
    padding: 1px 6px;
    white-space: nowrap;
  }
  .list-filter {
    min-width: 55px;
    width: 130px;
    padding: 1px 5px;
    flex: 0 1 130px;
  }
  .cue-count {
    color: var(--text-muted);
    white-space: nowrap;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .header-clip {
    overflow: hidden;
    flex-shrink: 0;
    height: 25px;
    background: var(--surface-raised);
  }
  .table-head,
  .cue-row {
    display: grid;
    grid-template-columns: var(--column-grid);
    min-width: var(--columns-min);
    gap: 6px;
    align-items: center;
    padding: 0 10px;
    text-align: left;
  }
  .table-head {
    width: 100%;
    height: 25px;
    color: var(--text-muted);
    font-size: 11px;
  }
  .column-heading {
    position: relative;
    height: 100%;
    display: flex;
    align-items: center;
    min-width: 0;
  }
  .column-heading > span {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .resize-edge {
    position: absolute;
    right: -4px;
    top: 0;
    width: 8px;
    height: 100%;
    padding: 0;
    border: 0;
    border-radius: 0;
    background: transparent;
    cursor: col-resize;
    touch-action: none;
  }
  .resize-edge:hover,
  .resize-edge:focus-visible {
    background: var(--accent-soft);
  }
  .cue-list {
    overflow: auto;
    flex: 1;
    min-height: 0;
  }
  .virtual-list {
    position: relative;
    min-width: var(--columns-min);
  }
  .cue-row {
    width: 100%;
    height: var(--row-height);
    border: 0;
    border-bottom: 1px solid var(--line);
    border-radius: 0;
    background: var(--surface-panel);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
  }
  .cue-row > span {
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .cue-row.selected {
    background: var(--selection);
    box-shadow: inset 3px 0 var(--accent);
  }
  .cue-row.active {
    border-inline-end: 3px solid var(--accent);
  }
  .cps {
    display: flex;
    flex-direction: column;
    gap: 1px;
    font-size: 11px;
  }
  .issue {
    font-weight: 700;
    color: var(--text-faint);
  }
  .issue.violation {
    color: var(--err);
  }
  .issue.advisory {
    color: var(--warn);
  }
  .empty {
    padding: 16px;
    color: var(--text-muted);
  }
  @container cue-table (max-width: 900px) {
    .cue-count {
      display: none;
    }
    .list-filter {
      flex: 1 1 80px;
      width: 80px;
    }
  }
</style>
