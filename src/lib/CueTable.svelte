<script lang="ts">
  import type { DocumentSession } from './document-session.svelte';
  import type { QualitySession } from './quality-session.svelte';
  import type { WorkspaceControls } from './workspace-controls';
  import { formatTime } from './timeline';
  import { rate } from './quality-display';
  import type { SelectionMode } from './cue-selection';
  import type { CueTableView } from './cue-table-view.svelte';
  let {
    subtitles,
    quality,
    tableView,
    workspace,
    metrics: showQualityMetrics,
    list = $bindable(),
    actions,
  }: {
    subtitles: DocumentSession;
    quality: QualitySession;
    tableView: CueTableView;
    workspace: WorkspaceControls;
    metrics: boolean;
    list: HTMLDivElement | undefined;
    actions: {
      suspendCueFollowing: () => void;
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
  const busy = $derived(workspace.busy);
  const firstRow = $derived(tableView.firstRow);
  const visibleCues = $derived(tableView.visibleCues);
  const activeIds = $derived(tableView.activeIds);
  const textInterpretation = $derived(
    doc.format?.name === 'vtt' ? 'webvtt_utf16_v1' : 'vss_utf16_v1',
  );
  const qualityRows = $derived(
    new Map(quality.view?.metrics.map((m) => [m.data.cueId, m]) ?? []),
  );
</script>

<div class="cue-table">
  <div class="table-head" class:with-metrics={showQualityMetrics}>
    <span>#</span><span>Start</span><span>End</span><span
      >Subtitle text · {doc.cues.length} cues · {subtitles.selectedIds.size} selected</span
    >
    {#if showQualityMetrics}<span>CPS · RS</span>{/if}
  </div>
  <div
    class="cue-list"
    bind:this={list}
    onscroll={(e) => (tableView.scrollTop = e.currentTarget.scrollTop)}
  >
    {#if !doc.loaded}<p class="empty">
        Create a cue to start new subtitles, or open SRT, ASS, SSA or WebVTT.
      </p>{:else if doc.cues.length === 0}<p class="empty">
        No cues yet. Create a cue to start editing.
      </p>{:else}
      <div style:height={`${doc.cues.length * 32}px`} class="virtual-list">
        <div style:transform={`translateY(${firstRow * 32}px)`}>
          {#each visibleCues as item, i (item.id)}<button
              class="cue-row"
              class:with-metrics={showQualityMetrics}
              class:selected={subtitles.selectedIds.has(item.id)}
              class:active={activeIds.includes(item.id)}
              aria-current={activeIds.includes(item.id) ? 'time' : undefined}
              onfocus={actions.suspendCueFollowing}
              aria-pressed={subtitles.selectedIds.has(item.id)}
              data-cue-id={item.id}
              aria-disabled={busy}
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
              ><span>{firstRow + i + 1}</span><span
                >{formatTime(item.startMs)}</span
              ><span>{formatTime(item.endMs)}</span><span class="cue-text"
                >{item.text.replaceAll('\n', ' ↵ ') || '(empty)'}</span
              >{#if showQualityMetrics}{@const metric = qualityRows.get(
                  String(item.id),
                )}<span
                  class="row-metrics"
                  title={`Accepted text · ${textInterpretation} · unrounded values drive checks`}
                  >{metric
                    ? `${rate(metric.data.cps)} · ${rate(metric.data.rs)}`
                    : '—'}</span
                >{/if}</button
            >{/each}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .cue-table {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
    min-height: 64px;
  }

  .table-head,
  .cue-row {
    display: grid;
    grid-template-columns: 44px 116px 116px minmax(0, 1fr);
    text-align: left;
    gap: 8px;
    align-items: center;
    padding: 0 10px;
  }

  .table-head {
    height: 25px;
    flex-shrink: 0;
    background: #304753;
    color: #bfd0da;
    font-size: 11px;
  }

  .cue-list {
    overflow: auto;
    flex: 1;
    min-height: 0;
  }

  .virtual-list {
    position: relative;
  }

  .cue-row {
    width: 100%;
    height: 32px;
    border: 0;
    border-bottom: 1px solid #29404d;
    border-radius: 0;
    background: #172934;
    font-variant-numeric: tabular-nums;
    font-size: 12px;
  }

  .cue-row.selected {
    background: #32566a;
    box-shadow: inset 3px 0 #80dfc8;
  }

  .table-head.with-metrics,
  .cue-row.with-metrics {
    grid-template-columns: 44px 116px 116px minmax(0, 1fr) 110px;
  }

  .row-metrics {
    color: #a4dccf;
    font-size: 11px;
  }

  :global(.with-problems) .table-head.with-metrics,
  :global(.with-problems) .cue-row.with-metrics {
    grid-template-columns: 26px 82px 82px minmax(60px, 1fr) 76px;
    gap: 5px;
    font-size: 11px;
  }

  .cue-row.active {
    border-inline-end: 4px solid #e8c76a;
  }

  .cue-row.active > span:first-child {
    color: #ffe39a;
    font-weight: 700;
  }

  .cue-text {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .empty {
    padding: 16px;
    color: #9cb2c1;
  }

  @media (max-width: 799px) {
    .table-head,
    .cue-row {
      grid-template-columns: 32px 100px 100px minmax(0, 1fr);
      gap: 4px;
      padding-inline: 6px;
    }

    .table-head.with-metrics,
    .cue-row.with-metrics {
      grid-template-columns: 26px 82px 82px minmax(60px, 1fr) 76px;
    }
  }
</style>
