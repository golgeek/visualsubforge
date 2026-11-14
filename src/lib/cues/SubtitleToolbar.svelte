<script lang="ts">
  import type { Workspace } from '../workspace/workspace.svelte';
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { WorkspaceDialogsState } from '../dialogs/workspace-dialogs.svelte';
  import type { WorkspaceView } from '../workspace/workspace-view.svelte';
  import type { QualitySession } from '../quality/quality-session.svelte';
  import type { WorkspacePreferences } from '../settings/workspace-preferences.svelte';
  import type { TimingGesture } from '../timing/timing-gesture.svelte';
  import type { CueNavigation } from './cue-navigation.svelte';
  import type { WorkspaceLayout } from '../layout/workspace-layout.svelte';
  let {
    workspace,
    subtitles,
    dialogs,
    view,
    quality,
    preferences,
    gestures,
    navigation,
    layout,
  }: {
    workspace: Workspace;
    subtitles: DocumentSession;
    dialogs: WorkspaceDialogsState;
    view: WorkspaceView;
    quality: QualitySession;
    preferences: WorkspacePreferences;
    gestures: TimingGesture;
    navigation: CueNavigation;
    layout: WorkspaceLayout;
  } = $props();
  import { measurePaneControl } from '../layout/measure-pane-control';
  import { setTimingMode } from '../workspace/view-actions';
  const doc = $derived(subtitles.snapshot);
  const draftDirty = $derived(subtitles.dirty);
  const reviews = [
    { label: 'Find / Replace…', open: () => dialogs.openText() },
    {
      label: 'Enforce minimum gaps…',
      open: () => dialogs.openTimingReview('gap'),
    },
    {
      label: 'Repair scene timing…',
      open: () => dialogs.openTimingReview('scene'),
    },
    {
      label: 'Repair duration / speed…',
      open: () => dialogs.openTimingReview('duration'),
    },
    {
      label: 'Shift / align cues…',
      open: () => dialogs.openTimingReview('batch'),
    },
  ];
</script>

<div
  class="cue-followbar"
  use:measurePaneControl={(height) => (layout.cueFollowingHeight = height)}
>
  {#each reviews as review (review.label)}
    <button
      disabled={workspace.busy || workspace.confirming || !doc.cues.length}
      onclick={() => void review.open()}>{review.label}</button
    >
  {/each}

  <button
    disabled={workspace.busy || workspace.confirming || !doc.loaded}
    onclick={() => void dialogs.openStatistics()}>Statistics…</button
  >
  <button
    disabled={workspace.busy || workspace.confirming || !doc.loaded}
    onclick={() => void dialogs.openAuthoring('plugins')}
    >Subtitle plugins…</button
  >
  <button
    aria-expanded={view.problemsOpen}
    onclick={() =>
      view.toggleProblems(() => {
        navigation.pendingProblem = null;
      })}
    >Problems{quality.view?.profileHash
      ? ` · ${quality.view.findingsCount}`
      : ''}</button
  >
  <button
    disabled={!workspace.canAct}
    aria-expanded={view.translationOpen}
    onclick={() => view.toggleTranslation()}
    >Translation reference{doc.translation?.dirty ? ' •' : ''}</button
  >
  <label
    ><input type="checkbox" bind:checked={preferences.showQualityMetrics} /> Cue metrics</label
  >
  <label
    title="Tap Space to play/pause. While playing, hold Space for at least 400 ms to create a cue and keep timing. Enter edits text; Escape returns to the waveform."
  >
    <input
      type="checkbox"
      checked={gestures.mode}
      disabled={workspace.busy || workspace.confirming}
      onchange={(e) =>
        setTimingMode(gestures, navigation, e.currentTarget.checked)}
    />
    Timing mode · hold Space
  </label>
  <label
    ><input
      type="checkbox"
      checked={navigation.following !== 'off'}
      onchange={(e) => {
        navigation.setFollowing(e.currentTarget.checked);
      }}
    /> Follow active subtitle</label
  >
  {#if navigation.following === 'suspended'}
    <span
      >{draftDirty
        ? 'Apply or revert the draft to resume.'
        : 'Following suspended.'}</span
    >
    <button
      disabled={draftDirty || workspace.busy || workspace.confirming}
      onclick={() => {
        navigation.resumeFollowing();
      }}>Resume following</button
    >
  {/if}
</div>

<style>
  .cue-followbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 4px 8px;
    font-size: 11px;
    background: var(--surface-raised);
    flex-shrink: 0;
  }

  .cue-followbar label {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .cue-followbar button {
    padding: 2px 6px;
  }
</style>
