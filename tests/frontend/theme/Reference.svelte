<script lang="ts">
  import '@fontsource-variable/inter/wght.css';
  import '@fontsource-variable/jetbrains-mono/wght.css';
  import '../../../src/lib/ui/tokens.css';
  import '../../../src/lib/ui/base.css';
  import Waveform from '../../../src/lib/timeline/Waveform.svelte';
  import type { Cue } from '../../../src/lib/client';

  let { tokens }: { tokens: string[] } = $props();
  const colors = $derived(
    tokens.filter((t) => !/^--(font|radius|control|motion|shadow)/.test(t)),
  );
  const cues: Cue[] = [
    { id: 1, startMs: 2000, endMs: 4200, text: 'Selected cue' },
    { id: 2, startMs: 4400, endMs: 6400, text: 'Next cue' },
  ];
  const noop = () => {};
</script>

<div class="page">
  <section aria-label="Tokens">
    <h2>Tokens</h2>
    <div class="swatches">
      {#each colors as token (token)}
        <div class="swatch">
          <span style:background={`var(${token})`}></span>
          <code>{token}</code>
        </div>
      {/each}
    </div>
  </section>

  <section aria-label="Controls">
    <h2>Controls</h2>
    <div class="row">
      <button type="button">Default</button>
      <button type="button" data-variant="primary">Save</button>
      <button type="button" data-variant="quiet">Quiet</button>
      <button type="button" data-variant="danger">Discard changes</button>
      <button type="button" disabled>Disabled</button>
      <kbd>⌘</kbd><kbd>K</kbd>
    </div>
    <div class="row">
      <input value="00:00:36.030" aria-label="Start" />
      <input placeholder="Placeholder" aria-label="Placeholder" />
      <select aria-label="Choice"><option>Netflix</option></select>
      <label><input type="checkbox" checked /> Snap</label>
      <label><input type="radio" checked /> Dark</label>
      <input type="range" aria-label="Zoom" />
      <progress value="0.6"></progress>
    </div>
    <textarea aria-label="Text" rows="2"
      >Two lines of subtitle text
for the selected cue</textarea
    >
  </section>

  <section aria-label="Status">
    <h2>Status and selection</h2>
    <p class="status">
      <span style:color="var(--ok)">Saved</span>
      <span style:color="var(--warn)">Near shot change</span>
      <span style:color="var(--err)">Reading speed too high</span>
      <span style:color="var(--accent-text)">Accent text</span>
      <span style:color="var(--text-muted)">Muted</span>
      <span style:color="var(--text-faint)">Faint</span>
    </p>
    <p class="chip" style:background="var(--err-soft)" style:color="var(--err)">
      Error on soft background
    </p>
    <p
      class="chip"
      style:background="var(--warn-soft)"
      style:color="var(--warn)"
    >
      Warning on soft background
    </p>
    <p class="selected">Selected row</p>
  </section>

  <section aria-label="Containers">
    <h2>Containers</h2>
    <dialog open>
      <strong>Dialog surface</strong>
      <p>Uses the panel surface, border and pop-up shadow.</p>
      <details open>
        <summary>Details</summary>
        <table>
          <thead><tr><th>Start</th><th>Text</th></tr></thead>
          <tbody><tr><td>00:00:36.030</td><td>Selected cue</td></tr></tbody>
        </table>
      </details>
    </dialog>
  </section>

  <section aria-label="Waveform">
    <h2>Waveform canvas</h2>
    <div class="wave">
      <Waveform
        view={null}
        start={0}
        span={10}
        position={5}
        duration={60}
        {cues}
        range={[7, 8.5]}
        selected={1}
        sceneCuts={[6.9]}
        onseek={noop}
        onrange={noop}
        onwidth={noop}
      />
    </div>
  </section>
</div>

<style>
  .page {
    display: grid;
    gap: 16px;
    padding: 16px;
    background: var(--surface-base);
    color: var(--text);
  }
  section {
    padding: 14px;
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    background: var(--surface-panel);
  }
  h2 {
    margin: 0 0 10px;
    font-size: var(--font-size-lg);
  }
  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 6px;
  }
  .swatch {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .swatch span {
    width: 26px;
    height: 18px;
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
  }
  textarea {
    width: 100%;
  }
  .status {
    display: flex;
    gap: 14px;
  }
  .chip {
    display: inline-block;
    margin: 0 8px 8px 0;
    padding: 3px 10px;
    border-radius: var(--radius-pill);
    font-weight: 600;
  }
  .selected {
    padding: 6px 10px;
    background: var(--selection);
    box-shadow: inset 3px 0 var(--accent);
  }
  dialog {
    position: static;
    margin: 0;
    width: min(420px, 100%);
  }
  .wave {
    height: 140px;
    width: 600px;
    max-width: 100%;
  }
</style>
