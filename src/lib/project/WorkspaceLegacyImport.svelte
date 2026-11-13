<script lang="ts">
  import type { LegacyImportDecision } from './legacy-import-decision.svelte';
  import type { MediaSession } from '../media/media-session.svelte';
  import type { ProjectWorkflow } from './project-workflow.svelte';
  let {
    legacyDecision,
    media,
    projects,
  }: {
    legacyDecision: LegacyImportDecision;
    media: MediaSession;
    projects: ProjectWorkflow;
  } = $props();
  import LegacyImportDialog from './LegacyImportDialog.svelte';
</script>

{#if legacyDecision.payload}
  <LegacyImportDialog
    canOpen={media.status.available}
    onclose={(path, open) => void legacyDecision.finish(projects, path, open)}
  />
{/if}

<style>
</style>
