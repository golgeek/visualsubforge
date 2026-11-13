<script lang="ts">
  import ResyncDialog from './ResyncDialog.svelte';
  import StatisticsDialog from './StatisticsDialog.svelte';
  import FontDialog from './FontDialog.svelte';
  import PluginDialog from './PluginDialog.svelte';
  import ConversionDialog from './ConversionDialog.svelte';
  import StyleInspector from './StyleInspector.svelte';
  import BatchTimingDialog from './BatchTimingDialog.svelte';
  import TextDialog from './TextDialog.svelte';
  import SettingsDialog from './SettingsDialog.svelte';
  import GapDialog from './GapDialog.svelte';
  import SceneRepairDialog from './SceneRepairDialog.svelte';
  import DurationRepairDialog from './DurationRepairDialog.svelte';
  import type { WorkspaceDialogsState } from './workspace-dialogs.svelte';
  import type { DocumentSession } from './document-session.svelte';
  import type { MediaSession } from './media-session.svelte';
  import type { Preferences } from './client';
  import type { TimingSettings } from './magnetic-timing';
  let {
    dialogs,
    subtitles,
    media,
    mediaPath,
    qualityContext,
    sceneState,
    preferences,
    timingSettings,
  }: {
    dialogs: WorkspaceDialogsState;
    subtitles: DocumentSession;
    media: MediaSession;
    mediaPath: string;
    qualityContext: number;
    sceneState: { qualityKey: string };
    preferences: Preferences;
    timingSettings: TimingSettings;
  } = $props();
  const workspaceModal = $derived(dialogs.modal);
  const doc = $derived(subtitles.snapshot);
  const status = $derived(media.status);
  const TimingDialog = $derived(
    workspaceModal.timingKind === 'duration'
      ? DurationRepairDialog
      : workspaceModal.timingKind === 'scene'
        ? SceneRepairDialog
        : GapDialog,
  );
</script>

{#if workspaceModal.identity === 'resync'}
  <ResyncDialog
    {subtitles}
    media={() => status}
    target={mediaPath}
    onclose={() => void dialogs.closeResync()}
    onaccepted={() => void dialogs.closeResync(true)}
  />
{/if}
{#if workspaceModal.identity === 'statistics'}
  <StatisticsDialog
    {subtitles}
    mediaSession={() => status.session}
    context={() => `${qualityContext}/${sceneState.qualityKey}`}
    onclose={() => void dialogs.closeStatistics()}
    onnavigate={(id) => void dialogs.closeStatistics(id)}
  />
{/if}
{#if workspaceModal.identity === 'fonts' && doc.format && subtitles.styled}
  <FontDialog
    {subtitles}
    onclose={() => void dialogs.closeAuthoring()}
    onaccepted={() => void dialogs.closeAuthoring(true)}
  />
{/if}
{#if workspaceModal.identity === 'plugins'}
  <PluginDialog
    {subtitles}
    media={() => status}
    context={() => `${qualityContext}/${sceneState.qualityKey}`}
    onclose={() => void dialogs.closeAuthoring()}
    onaccepted={() => void dialogs.closeAuthoring(true)}
  />
{/if}
{#if workspaceModal.identity === 'conversion'}
  <ConversionDialog
    {subtitles}
    onclose={(path) => void dialogs.closeAuthoring(false, path)}
  />
{/if}

{#if workspaceModal.identity === 'styles' && doc.format && subtitles.styled}
  <StyleInspector
    {subtitles}
    initial={subtitles.style}
    onclose={() => void dialogs.closeAuthoring()}
    onaccepted={() => void dialogs.closeAuthoring(true)}
  />
{/if}
{#if workspaceModal.timingKind !== null}
  {#if workspaceModal.timingKind === 'batch'}
    <BatchTimingDialog
      {subtitles}
      media={() => status}
      onclose={() => void dialogs.closeTimingReview()}
      onaccepted={() => void dialogs.closeTimingReview(true)}
    />
  {:else}
    <TimingDialog
      {subtitles}
      mediaSession={() => status.session}
      onclose={() => void dialogs.closeTimingReview()}
      onaccepted={() => void dialogs.closeTimingReview(true)}
    />
  {/if}
{/if}
{#if workspaceModal.identity === 'text'}
  <TextDialog
    {subtitles}
    initial={dialogs.textOperation}
    onclose={(operation) => void dialogs.closeText(operation)}
    onaccepted={(operation) => void dialogs.closeText(operation, true)}
    onnavigate={(id, start, end, operation) =>
      void dialogs.navigateText(id, start, end, operation)}
  />
{/if}
{#if workspaceModal.identity === 'settings'}
  <SettingsDialog
    {preferences}
    timing={timingSettings}
    onapply={(value) => void dialogs.closeSettings(value)}
    onclose={() => void dialogs.closeSettings()}
  />
{/if}
