<script lang="ts">
  import type { Workspace } from '../workspace/workspace.svelte';
  import type { DocumentSession } from '../document/document-session.svelte';
  import type { WorkspacePreferences } from '../settings/workspace-preferences.svelte';
  import type { CueNavigation } from './cue-navigation.svelte';
  let {
    workspace,
    subtitles,
    preferences,
    navigation,
  }: {
    workspace: Workspace;
    subtitles: DocumentSession;
    preferences: WorkspacePreferences;
    navigation: CueNavigation;
  } = $props();
  const draftDirty = $derived(subtitles.dirty);
</script>

<div class="cue-followbar">
  <label
    ><input
      aria-label="Cue metrics"
      type="checkbox"
      bind:checked={preferences.showQualityMetrics}
    /> <span class="compact-extra">Cue </span>metrics</label
  >
  <label
    ><input
      type="checkbox"
      aria-label="Follow playhead"
      checked={navigation.following !== 'off'}
      onchange={(e) => {
        navigation.setFollowing(e.currentTarget.checked);
      }}
    />
    Follow<span class="compact-extra"> playhead</span></label
  >
  {#if navigation.following === 'suspended'}
    <span
      >{draftDirty
        ? 'Apply or revert your edit to follow playback.'
        : 'Following paused.'}</span
    >
    <button
      aria-label="Jump to playhead"
      disabled={draftDirty || workspace.busy || workspace.confirming}
      onclick={() => {
        navigation.resumeFollowing();
      }}>Jump<span class="compact-extra">&nbsp;to playhead</span></button
    >
  {/if}
</div>

<style>
  .cue-followbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0;
    font-size: 11px;
    background: transparent;
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
  .cue-followbar > span {
    max-width: 90px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cue-followbar label {
    white-space: nowrap;
  }
  @container cue-table (max-width: 900px) {
    .compact-extra,
    .cue-followbar > span {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }
  }
</style>
