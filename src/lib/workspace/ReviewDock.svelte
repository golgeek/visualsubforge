<script lang="ts">
  import { onMount, type Snippet } from 'svelte';
  import { formatTime } from '../timeline/timeline';
  import type { WorkspaceView } from './workspace-view.svelte';
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { CueTableView } from '../cues/cue-table-view.svelte';
  import type { TimingPreview } from '../timing/review-preview';
  import ReviewTiming from '../timing/ReviewTiming.svelte';
  import StatisticsPanel from '../quality/StatisticsPanel.svelte';
  let {
    view,
    subtitles,
    tableView,
    mediaSession,
    playhead,
    context,
    previewIds,
    enabled,
    issues,
    onclose,
    onpreview,
    onaccepted,
    onnavigate,
  }: {
    view: WorkspaceView;
    subtitles: DocumentSession;
    tableView: CueTableView;
    mediaSession: () => number;
    playhead: () => number | null;
    context: () => string;
    previewIds: () => readonly number[];
    enabled: () => boolean;
    issues: Snippet;
    onclose: () => void;
    onpreview: (value: TimingPreview | null) => void;
    onaccepted: () => void;
    onnavigate: (id: string) => void;
  } = $props();
  onMount(() => {
    const launcher =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    return () => {
      if (launcher?.isConnected && !launcher.matches(':disabled'))
        launcher.focus({ preventScroll: true });
    };
  });
  const busy = $derived(view.reviewBusy);
  let scopePlayhead = $state<number | null>(null);
  let scope = $state<
    'all' | 'selected' | 'filtered' | 'following' | 'playhead'
  >('all');
  $effect(() => {
    if (
      view.reviewTab !== 'shift' &&
      (scope === 'following' || scope === 'playhead')
    )
      scope = 'all';
  });
  const scopeIds = () => {
    if (scope === 'all') return null;
    if (scope === 'selected') return [...subtitles.selectedIds];
    if (scope === 'following') {
      const index = subtitles.snapshot.cues.findIndex(
        (cue) => cue.id === subtitles.selected,
      );
      return index < 0
        ? []
        : subtitles.snapshot.cues.slice(index).map((cue) => cue.id);
    }
    if (scope === 'playhead') {
      const position = scopePlayhead;
      return position === null
        ? []
        : subtitles.snapshot.cues
            .filter((cue) => cue.startMs >= position)
            .map((cue) => cue.id);
    }
    return tableView.order.map((cue) => cue.id);
  };
  const tabs = [
    { id: 'issues', label: 'Issues' },
    { id: 'timing', label: 'Fix timing' },
    { id: 'shift', label: 'Shift' },
    { id: 'statistics', label: 'Statistics' },
  ] as const;
</script>

<aside class="review-dock" aria-label="Review panel">
  <header>
    <strong>Review</strong><button
      disabled={busy}
      aria-label="Close review panel"
      onclick={onclose}>Close</button
    >
  </header>
  <nav aria-label="Review tools">
    {#each tabs as tab (tab.id)}<button
        disabled={busy}
        aria-pressed={view.reviewTab === tab.id}
        onclick={() => (view.reviewTab = tab.id)}>{tab.label}</button
      >{/each}
  </nav>
  <div class="review-options">
    {#if view.reviewTab !== 'issues'}<label class="scope"
        >Scope <select
          disabled={busy}
          aria-label="Review scope"
          onchange={(event) => {
            if (event.currentTarget.value === 'playhead')
              scopePlayhead = playhead();
          }}
          bind:value={scope}
          ><option value="all">Whole file</option><option value="selected"
            >List selection</option
          ><option value="filtered">Filtered list</option
          >{#if view.reviewTab === 'shift'}<option value="following"
              >Current cue onward</option
            ><option value="playhead" disabled={playhead() === null}
              >At or after paused playhead</option
            >{/if}</select
        ></label
      >{/if}
    {#if view.reviewTab === 'shift' && scope === 'playhead'}<div class="scope">
        <span
          >From {scopePlayhead === null ? '—' : formatTime(scopePlayhead)}</span
        ><button
          disabled={busy || playhead() === null}
          onclick={() => (scopePlayhead = playhead())}
          >Capture scope playhead</button
        >
      </div>{/if}
    {#if view.reviewTab === 'timing'}<label class="scope"
        >Repair <select
          disabled={busy}
          aria-label="Timing repair"
          bind:value={view.timingKind}
          ><option value="gap">Gaps and overlaps</option><option
            value="duration">Reading speed</option
          ><option value="scene">Shot changes</option></select
        ></label
      >{/if}
  </div>
  <div class="body">
    {#if view.reviewTab === 'issues'}{@render issues()}
    {:else if view.reviewTab === 'statistics'}<StatisticsPanel
        {subtitles}
        {mediaSession}
        {context}
        {scopeIds}
        {enabled}
        {onclose}
        {onnavigate}
      />
    {:else}{#key view.reviewTab === 'shift' ? 'batch' : view.timingKind}<ReviewTiming
          kind={view.reviewTab === 'shift' ? 'batch' : view.timingKind}
          {subtitles}
          {mediaSession}
          {context}
          {scopeIds}
          {previewIds}
          {playhead}
          onbusy={(value) => (view.reviewBusy = value)}
          {enabled}
          {onpreview}
          {onaccepted}
        />{/key}{/if}
  </div>
</aside>

<style>
  .review-dock {
    flex: 0 0 42%;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--line);
    background: var(--surface-panel);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 10px;
  }
  nav {
    display: flex;
    gap: 2px;
    padding: 0 6px 6px;
    border-bottom: 1px solid var(--line);
  }
  nav button {
    flex: 1;
    padding: 5px 2px;
    font-size: 11px;
  }
  nav button[aria-pressed='true'] {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .review-options {
    display: flex;
    gap: 8px;
    padding: 5px 8px;
  }
  .review-options:empty {
    display: none;
  }
  .scope {
    flex: 1;
    min-width: 0;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 0;
    font-size: 11px;
  }
  select {
    min-width: 0;
    flex: 1;
    width: 0;
  }
  .body {
    overflow: auto;
    min-height: 0;
    flex: 1;
  }
</style>
