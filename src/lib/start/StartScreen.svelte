<script lang="ts">
  import RecoveryCard from '../document/RecoveryCard.svelte';
  import type { Recovery } from '../document/recovery.svelte';
  import type { Workspace } from '../workspace/workspace.svelte';
  let {
    workspace,
    recovery,
    onopen,
    onnew,
    onimport,
  }: {
    workspace: Workspace;
    recovery: Recovery;
    onopen: () => Promise<void>;
    onnew: () => Promise<void>;
    onimport: () => Promise<void>;
  } = $props();
</script>

<section class="start-screen" aria-label="Start subtitling">
  <div class="welcome">
    <h1>Start subtitling</h1>
    <p class="lead">
      Open a video and its subtitles, or start a new file and time it against
      the waveform.
    </p>
    <button
      class="drop"
      disabled={!workspace.canAct}
      onclick={() => void onopen()}
    >
      <strong>Drop a video, subtitles or a project</strong>
      <span
        >MP4, MKV, MOV · SRT, ASS, SSA, VTT · .vsfproject · VisualSubSync
        .vssprj</span
      >
      <span>Or click to choose a file</span>
    </button>
    <div class="actions">
      <button
        data-variant="primary"
        disabled={!workspace.canAct}
        onclick={() => void onopen()}>Open…</button
      >
      <button disabled={!workspace.canAct} onclick={() => void onnew()}
        >New subtitles</button
      >
      <button disabled={!workspace.canAct} onclick={() => void onimport()}
        >Import VisualSubSync project</button
      >
    </div>
  </div>
  <aside class="start-side">
    <section class="recent" aria-label="Recent work">
      <h2>Recent work</h2>
      <p>Opened work will appear here.</p>
    </section>
    <RecoveryCard {recovery} {workspace} />
  </aside>
</section>

<style>
  .start-screen {
    display: grid;
    grid-template-columns: minmax(0, 1.3fr) minmax(280px, 1fr);
    gap: 40px;
    align-content: center;
    padding: clamp(24px, 5vw, 72px);
    z-index: 2;
    overflow: auto;
  }
  h1 {
    font-size: 32px;
    margin: 0 0 16px;
  }
  .lead {
    color: var(--text-muted);
    font-size: 16px;
    line-height: 1.6;
    max-width: 44ch;
  }
  .drop {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    white-space: normal;
    width: 100%;
    min-height: 200px;
    padding: 24px;
    gap: 14px;
    border: 2px dashed var(--line-strong);
    border-radius: var(--radius-lg);
    background: var(--surface-panel);
  }
  .drop strong {
    font-size: 18px;
  }
  .drop span {
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 20px;
  }
  .start-side {
    display: flex;
    flex-direction: column;
    gap: 24px;
  }
  .recent {
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    padding: 16px;
    background: var(--surface-panel);
  }
  h2 {
    margin: 0 0 12px;
    font-size: 16px;
  }
  .recent p {
    color: var(--text-muted);
  }
  @media (max-width: 799px) {
    .start-screen {
      grid-template-columns: 1fr;
      align-content: start;
      gap: 24px;
    }
  }
</style>
