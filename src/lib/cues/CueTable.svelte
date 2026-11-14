<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { QualitySession } from '../quality/quality-session.svelte';
  import type { Workspace } from '../workspace/workspace.svelte';
  import type { WorkspaceLayout } from '../layout/workspace-layout.svelte';
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
    subtitles,
    quality,
    tableView,
    workspace,
    layout,
    list = $bindable(),
    actions,
    controls,
  }: {
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
        inRange: boolean,
      ) => Promise<void>;
      selectCue: (id: number, mode?: SelectionMode) => Promise<void>;
      cueSelectionKey: (event: KeyboardEvent, id: number) => void;
      focusText: () => Promise<void>;
    };
  } = $props();
  const doc = $derived(subtitles.snapshot);
  const columns = $derived(tableView.visibleColumns);
  const minimumWidth = $derived(
    columns.reduce(
      (sum, c) => sum + c.width,
      20 + Math.max(0, columns.length - 1) * 6,
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
  $effect(() => {
    void tableView.filterKey;
    void doc.generation;
    void layout.preset;
    if (list) {
      list.scrollTop = 0;
      tableView.scrollTop = 0;
    }
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

<div
  class="cue-table"
  style:--column-grid={columnGrid(columns)}
  style:--columns-min={`${minimumWidth}px`}
  style:--row-height={`${tableView.rowHeight}px`}
>
  <div class="list-controls">
    <button disabled={!workspace.canAct} onclick={actions.columns}
      >Columns ▾</button
    >
    <input
      class="list-filter"
      aria-label="Filter cues by text or number"
      placeholder="Text or #"
      bind:value={tableView.query}
    />
    <Chip
      active={tableView.issuesOnly}
      onclick={() => (tableView.issuesOnly = !tableView.issuesOnly)}
      >Issues {quality.issueIndexComplete ? quality.issues.size : '…'}</Chip
    >
    <span class="cue-count"
      >{tableView.rows.length} / {doc.cues.length} · {subtitles.selectedIds
        .size} selected</span
    >
    {@render controls?.()}
  </div>
  {#if quality.error || quality.projectionError}<div
      class="check-error"
      role="alert"
    >
      Checks unavailable: {quality.error || quality.projectionError}
    </div>{/if}
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
            {@const metric = quality.metrics.get(String(item.id))}
            {@const issue = quality.issues.get(String(item.id))}
            <button
              class="cue-row"
              class:selected={subtitles.selectedIds.has(item.id)}
              class:active={activeIds.includes(item.id)}
              aria-current={activeIds.includes(item.id) ? 'time' : undefined}
              aria-pressed={subtitles.selectedIds.has(item.id)}
              data-cue-id={item.id}
              aria-disabled={workspace.busy}
              oncontextmenu={(e) => {
                e.preventDefault();
                void actions.openCueMenu(e.clientX, e.clientY, item.id, false);
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
              onkeydown={(e) => actions.cueSelectionKey(e, item.id)}
              ondblclick={() => void actions.focusText()}
            >
              {#each columns as column (column.id)}
                {#if column.id === 'issue'}<span
                    class="issue"
                    class:violation={issue === 'violation'}
                    class:advisory={issue === 'advisory'}
                    aria-label={issue === 'violation'
                      ? 'Error'
                      : issue === 'advisory'
                        ? 'Warning'
                        : quality.issueIndexComplete
                          ? 'No reported issue'
                          : 'Issues not yet available'}
                    >{issue === 'violation'
                      ? '!'
                      : issue === 'advisory'
                        ? '•'
                        : quality.issueIndexComplete
                          ? ''
                          : '…'}</span
                  >
                {:else if column.id === 'cps'}<span class="cps" data-cell="cps"
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
                    data-cell={column.id}>{cell(row, column.id, metric)}</span
                  >{/if}
              {/each}
            </button>
          {/each}
        </div>
      </div>{/if}
  </div>
</div>

<style>
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
  .issue.violation,
  .check-error {
    color: var(--err);
  }
  .issue.advisory {
    color: var(--warn);
  }
  .check-error {
    font-size: 11px;
    padding: 2px 8px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
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
