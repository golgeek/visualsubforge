<script lang="ts">
  import { onMount, tick, type Snippet } from 'svelte';
  import { formatTime } from '../timeline/timeline';
  import type { WorkspaceView } from './workspace-view.svelte';
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { CueTableView } from '../cues/cue-table-view.svelte';
  import type { TimingPreview } from '../timing/review-preview';
  import type { TimingFixOptions } from '../client';
  import ReviewShift from '../timing/ReviewShift.svelte';
  import ReviewTiming from '../timing/ReviewTiming.svelte';
  import StatisticsPanel from '../quality/StatisticsPanel.svelte';
  import type { WorkspaceActivity } from '../workspace/workspace-activity.svelte';
  import { TimingReviewSelection } from '../timing/timing-review-selection.svelte';
  type Navigate = (id: string, origin: HTMLButtonElement) => void;
  let {
    activity,
    view,
    options,
    onoptions,
    issueCount,
    subtitles,
    tableView,
    mediaSession,
    playhead,
    context,
    previewIds,
    enabled,
    navigating,
    issues,
    changes,
    onclose,
    onpreview,
    onaccepted,
    onnavigate,
  }: {
    activity: WorkspaceActivity;
    view: WorkspaceView;
    options: TimingFixOptions;
    onoptions: (o: TimingFixOptions) => void;
    issueCount: number;
    subtitles: DocumentSession;
    tableView: CueTableView;
    mediaSession: () => number;
    playhead: () => number | null;
    context: () => string;
    previewIds: () => readonly number[];
    enabled: () => boolean;
    navigating: () => boolean;
    issues: Snippet<[Navigate]>;
    changes: Snippet;
    onclose: () => void;
    onpreview: (value: TimingPreview | null) => void;
    onaccepted: () => void;
    onnavigate: (id: string) => Promise<void>;
  } = $props();
  let panel: HTMLElement;
  let focusRequest: AbortController | undefined;
  function preserveDraftFocus(event: MouseEvent) {
    // Blur can commit and disable or replace the link before its click fires.
    if (
      event.button === 0 &&
      subtitles.dirty &&
      event.target instanceof Element &&
      event.target.closest('button[data-nav-key]')
    )
      event.preventDefault();
  }
  const navigate: Navigate = async (id, button) => {
    focusRequest?.abort();
    const request = new AbortController();
    focusRequest = request;
    const stop = () => request.abort();
    const options = { capture: true, signal: request.signal };
    document.addEventListener('pointerdown', stop, options);
    document.addEventListener('keydown', stop, options);
    await onnavigate(id);
    await tick();
    if (request.signal.aborted) return;
    if (subtitles.dirty) {
      stop();
      return;
    }
    const key = button.dataset.navKey!;
    const selector = `button[data-nav-key="${CSS.escape(key)}"]:enabled`;
    const restore = () => {
      const replacement = panel.querySelector<HTMLButtonElement>(selector);
      if (!replacement || replacement.closest('[inert]')) return false;
      replacement.focus({ preventScroll: true });
      stop();
      return true;
    };
    if (restore()) return;
    // A new accepted revision can replace the button until the fresh report arrives.
    panel
      .querySelector<HTMLButtonElement>('nav button[aria-pressed=true]')
      ?.focus({ preventScroll: true });
    const observer = new MutationObserver(restore);
    request.signal.addEventListener('abort', () => observer.disconnect(), {
      once: true,
    });
    observer.observe(panel, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ['inert', 'disabled'],
    });
  };
  onMount(() => {
    const launcher =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    panel.addEventListener('mousedown', preserveDraftFocus);
    return () => {
      focusRequest?.abort();
      panel.removeEventListener('mousedown', preserveDraftFocus);
      view.timingSelection = new TimingReviewSelection();
      if (launcher?.isConnected && !launcher.matches(':disabled'))
        launcher.focus({ preventScroll: true });
    };
  });
  const busy = $derived(view.reviewBusy);
  let scopePlayhead = $state<number | null>(null);
  let scope = $state<
    'all' | 'selected' | 'filtered' | 'following' | 'playhead' | 'current'
  >('all');
  $effect(() => {
    if (
      !(view.reviewTab === 'timing' && view.timingMode === 'shift') &&
      (scope === 'following' || scope === 'playhead')
    )
      scope = 'all';
    if (
      scope === 'current' &&
      (view.reviewTab !== 'timing' || view.timingMode === 'shift')
    )
      scope = 'all';
  });
  const scopeIds = () => {
    if (scope === 'all') return null;
    if (scope === 'current')
      return subtitles.selected === null ? [] : [subtitles.selected];
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
  $effect.pre(() => {
    const key = JSON.stringify([
      subtitles.snapshot.generation,
      subtitles.snapshot.revision,
      mediaSession(),
      scopeIds(),
      context(),
      view.families,
      options,
    ]);
    if (view.timingSelection.selectionKey !== key)
      view.timingSelection = new TimingReviewSelection(key);
  });
  const tabs = [
    { id: 'issues', label: 'Issues' },
    { id: 'timing', label: 'Timing' },
    { id: 'statistics', label: 'Statistics' },
    { id: 'changes', label: 'Changes' },
  ] as const;
</script>

<aside bind:this={panel} class="review-dock" aria-label="Review panel">
  <header>
    <strong>Review</strong>
    <nav aria-label="Review tools">
      {#each tabs as tab (tab.id)}<button
          disabled={busy}
          aria-pressed={view.reviewTab === tab.id}
          onclick={() => (view.reviewTab = tab.id)}
          >{tab.label}{tab.id === 'issues' ? ' ' + issueCount : ''}</button
        >{/each}
    </nav>
    <button
      data-variant="quiet"
      disabled={busy}
      aria-label="Close review panel"
      onclick={onclose}>Close</button
    >
  </header>
  <div class="review-options">
    {#if view.reviewTab === 'timing'}<div
        class="mode"
        role="radiogroup"
        aria-label="Timing mode"
      >
        {#each ['fix', 'shift'] as const as mode (mode)}<label
            ><input
              type="radio"
              name="timing-mode"
              checked={view.timingMode === mode}
              disabled={busy}
              onchange={() => (view.timingMode = mode)}
            />{mode === 'fix' ? 'Fix' : 'Shift'}</label
          >{/each}
      </div>{/if}
    {#if view.reviewTab !== 'issues' && view.reviewTab !== 'changes'}<label
        class="scope"
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
          >{#if view.reviewTab === 'timing' && view.timingMode === 'shift'}<option
              value="following">Current cue onward</option
            ><option value="playhead" disabled={playhead() === null}
              >At or after paused playhead</option
            >{:else if view.reviewTab === 'timing'}<option value="current"
              >Current cue</option
            >{/if}</select
        ></label
      >{/if}
    {#if view.reviewTab === 'timing' && view.timingMode === 'shift' && scope === 'playhead'}<div
        class="scope"
      >
        <span
          >From {scopePlayhead === null ? '—' : formatTime(scopePlayhead)}</span
        ><button
          disabled={busy || playhead() === null}
          onclick={() => (scopePlayhead = playhead())}
          >Capture scope playhead</button
        >
      </div>{/if}
    {#if view.reviewTab === 'timing' && view.timingMode === 'fix'}<button
        class="policy-button"
        data-variant="quiet"
        aria-expanded={view.policyOpen}
        onclick={() => (view.policyOpen = !view.policyOpen)}
        >{options.priority === 'spacing'
          ? 'Keep spacing'
          : 'Keep reading speed'} · {options.startToleranceMs
          ? 'starts ±' + options.startToleranceMs + ' ms'
          : 'start moves off'} · {options.sceneEdges === 'nearest'
          ? 'nearest edge'
          : 'inward'} ▾</button
      >{/if}
  </div>
  <div class="body">
    {#if view.reviewTab === 'issues'}{@render issues(navigate)}
    {:else if view.reviewTab === 'changes'}{@render changes()}
    {:else if view.reviewTab === 'statistics'}<StatisticsPanel
        {subtitles}
        {mediaSession}
        {context}
        {scopeIds}
        enabled={() => enabled() || navigating()}
        {onclose}
        onnavigate={navigate}
      />
    {:else if view.timingMode === 'shift'}<ReviewShift
        {activity}
        {subtitles}
        {mediaSession}
        {context}
        {scopeIds}
        {previewIds}
        {playhead}
        onbusy={(v) => (view.reviewBusy = v)}
        {enabled}
        {onpreview}
        {onaccepted}
      />
    {:else}<ReviewTiming
        {activity}
        previewEnabled={() => enabled() || navigating()}
        {view}
        {subtitles}
        {mediaSession}
        {context}
        {scopeIds}
        {previewIds}
        {options}
        {onoptions}
        onnavigate={navigate}
        onbusy={(v) => (view.reviewBusy = v)}
        {enabled}
        {onpreview}
        {onaccepted}
      />{/if}
  </div>
</aside>

<style>
  .review-dock {
    container-type: inline-size;
    flex: 0 0 42%;
    overflow: auto;
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
    flex: 1 1 150px;
    min-width: min(150px, 100%);
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    padding: 0;
    font-size: 11px;
  }
  select {
    min-width: 90px;
    flex: 1;
    width: auto;
  }
  .body {
    overflow: auto;
    min-height: 175px;
    flex: 1;
  }
  header,
  .review-options {
    flex-wrap: wrap;
  }
  .policy-button {
    margin-inline-start: auto;
    white-space: normal;
    text-align: end;
  }
  .mode {
    display: inline-flex;
  }
  .mode label {
    position: relative;
    cursor: pointer;
    border: 1px solid var(--line-strong);
    padding: var(--control-pad-y) var(--control-pad-x);
  }
  .mode label:has(:checked) {
    color: var(--accent-text);
    background: var(--accent-soft);
  }
  .mode input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    cursor: pointer;
    opacity: 0;
  }
  .mode label:focus-within {
    outline: 2px solid var(--focus);
  }
  @container (max-width: 360px) {
    header {
      justify-content: flex-start;
      gap: 4px;
    }
    header strong {
      flex-basis: 100%;
    }
    nav {
      padding: 0;
      border: 0;
    }
    .policy-button {
      margin-inline-start: 0;
      text-align: left;
    }
  }
</style>
