<script lang="ts">
  import '@fontsource-variable/inter/wght.css';
  import '@fontsource-variable/jetbrains-mono/wght.css';
  import '../../../src/lib/ui/tokens.css';
  import '../../../src/lib/ui/base.css';
  import Waveform from '../../../src/lib/timeline/Waveform.svelte';
  import TimelineStrip from '../../../src/lib/timeline/TimelineStrip.svelte';
  import { defaultTiming } from '../../../src/lib/timing/magnetic-timing';
  const cues = [
    { id: 120, startMs: 36000, endMs: 37920, text: 'A line before the cut.' },
    {
      id: 140,
      startMs: 37970,
      endMs: 42325,
      text: 'The next line crosses a shot.',
    },
    { id: 150, startMs: 42280, endMs: 44000, text: 'Overlapping cue' },
  ];
  const cuts = [37.98, 42.4],
    numbers = new Map([
      [120, 12],
      [140, 14],
      [150, 15],
    ]);
  const issues = new Map([
    ['140', 'advisory'],
    ['150', 'violation'],
  ] as const);
  const timing = { ...defaultTiming, enabled: true };
  const noop = () => {};
</script>

<main>
  <h1>Timeline marks</h1>
  <p>Modeled cue and cut data. No native media or detection-accuracy claim.</p>
  <TimelineStrip
    overview
    {cues}
    {cuts}
    {issues}
    start={36}
    span={10}
    duration={60}
    selected={140}
    position={38.7}
    enabled={false}
    mediaKey={1}
    onmove={noop}
  />
  {#each ['At rest', 'Dragging a cue edge', 'Zoomed cut distance'] as label, index (label)}
    <section>
      <h2>{label}</h2>
      <div class="wave">
        <Waveform
          view={null}
          start={index === 2 ? 41.9 : 36}
          span={index === 2 ? 1 : 10}
          duration={60}
          position={index === 2 ? null : 38.7}
          {cues}
          sceneCuts={cuts}
          cueNumbers={numbers}
          {issues}
          {timing}
          selected={140}
          range={null}
          editingCueId={index === 1 ? 140 : null}
          externalGuide={index === 1
            ? { milliseconds: 38110, label: '130 ms after shot change' }
            : null}
          onseek={noop}
          onrange={noop}
          onwidth={noop}
        />
      </div>
    </section>
  {/each}
  <h2>Same-window video scrubber</h2>
  <TimelineStrip
    {cues}
    {cuts}
    start={36}
    span={10}
    duration={60}
    selected={140}
    position={38.7}
    enabled={false}
    mediaKey={1}
    onmove={noop}
  />
</main>

<style>
  main {
    max-width: 1000px;
    margin: 24px auto;
    padding: 20px;
    color: var(--text);
    background: var(--surface-base);
    font-family: var(--font-ui);
  }
  h1 {
    font-size: 22px;
  }
  h2 {
    font-size: 14px;
    margin: 20px 0 6px;
  }
  p {
    color: var(--text-muted);
  }
  .wave {
    position: relative;
    height: 130px;
  }
</style>
