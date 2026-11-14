<script lang="ts">
  import { onMount, tick } from 'svelte';
  import '../../../src/lib/ui/tokens.css';
  import TimelineStrip from '../../../src/lib/timeline/TimelineStrip.svelte';
  import { SceneOverview } from '../../../src/lib/timeline/scene-overview.svelte';
  import type { SceneView } from '../../../src/lib/client';
  let results = $state<string[]>([]),
    complete = $state(false);
  let start = $state(10),
    span = $state(20),
    session = $state(1),
    enabled = $state(true),
    width = $state(600);
  let mounted = $state(true),
    host: HTMLDivElement;
  let seeks: number[] = [],
    active = false;
  const cues = [{ id: 500, startMs: 10000, endMs: 18000, text: 'Selected' }];
  function assert(value: unknown, message: string) {
    if (!value) throw new Error(message);
  }
  const paint = async () => {
    await tick();
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    );
  };
  function canvas(label: string) {
    return host.querySelector<HTMLCanvasElement>(`[aria-label="${label}"]`)!;
  }
  function capture(element: HTMLCanvasElement) {
    let pointer: number | null = null;
    element.setPointerCapture = (id) => {
      pointer = id;
    };
    element.hasPointerCapture = (id) => pointer === id;
    element.releasePointerCapture = () => {
      pointer = null;
    };
  }
  async function pointer(label: string, type: string, x: number) {
    const element = canvas(label);
    element.dispatchEvent(
      new PointerEvent(type, {
        pointerId: 1,
        isPrimary: true,
        button: 0,
        bubbles: true,
        clientX: element.getBoundingClientRect().left + x,
      }),
    );
    await tick();
  }
  const view = (
    generation: number,
    cuts: number[],
    truncated = false,
  ): SceneView => ({
    generation,
    mode: 'visual',
    scanning: null,
    running: false,
    error: null,
    total: 4,
    truncated,
    cuts,
    previous: null,
    next: null,
    phase: 'full',
    progress: 1,
    partial: false,
    note: null,
  });
  async function settle(check: () => boolean) {
    for (let i = 0; i < 100 && !check(); i++)
      await new Promise((resolve) => setTimeout(resolve, 5));
    assert(check(), 'Read did not settle');
  }
  async function run() {
    try {
      let calls = 0;
      const overview = new SceneOverview(async (generation, left, right) => {
        calls++;
        return view(
          generation,
          [1, 2, 8, 9].filter((c) => c >= left && c <= right).slice(0, 2),
          right - left > 5,
        );
      });
      overview.configure({
        session: 1,
        generation: 1,
        duration: 10,
        total: 4,
        partial: false,
      });
      await settle(() => !overview.incomplete);
      assert(
        JSON.stringify(overview.cuts) === '[1,2,8,9]' && calls === 3,
        'Truncated full-file read must subdivide and deduplicate',
      );
      overview.configure({
        session: 1,
        generation: 1,
        duration: 10,
        total: 4,
        partial: false,
      });
      await paint();
      assert(
        calls === 3,
        'Unchanged scene context must not re-read on viewport polling',
      );
      overview.dispose();
      results.push(
        'PASS: full-file cut reads subdivide bounded windows independently of viewport',
      );
      let release: ((value: SceneView) => void) | undefined;
      let inFlight = 0,
        maximum = 0;
      const late = new SceneOverview(async (generation) => {
        inFlight++;
        maximum = Math.max(maximum, inFlight);
        const response = await new Promise<SceneView>(
          (resolve) => (release = resolve),
        );
        inFlight--;
        return { ...response, generation };
      });
      late.configure({
        session: 1,
        generation: 1,
        duration: 10,
        total: 1,
        partial: false,
      });
      late.configure({
        session: 2,
        generation: 2,
        duration: 10,
        total: 1,
        partial: false,
      });
      release!(view(1, [1]));
      await settle(() => inFlight === 1 && !!release);
      await paint();
      assert(
        !late.cuts.length && maximum === 1,
        'Replacement must discard stale cuts and serialize reads',
      );
      release!(view(2, [8]));
      await settle(() => late.cuts[0] === 8);
      late.dispose();
      results.push(
        'PASS: media replacement discards late cuts and keeps one read in flight',
      );
      let boundedCalls = 0;
      const bounded = new SceneOverview(async (generation) => {
        boundedCalls++;
        return view(generation, [0], true);
      });
      bounded.configure({
        session: 1,
        generation: 1,
        duration: 1e6,
        total: 100000,
        partial: false,
      });
      await paint();
      await new Promise((resolve) => setTimeout(resolve, 20));
      assert(
        bounded.incomplete && boundedCalls === 128,
        'Bounded or unresolvable coverage must never be labelled complete',
      );
      bounded.dispose();
      const disposed = new SceneOverview(
        () => new Promise((resolve) => (release = resolve)),
      );
      disposed.configure({
        session: 1,
        generation: 1,
        duration: 10,
        total: 1,
        partial: false,
      });
      disposed.dispose();
      release!(view(1, [2]));
      await paint();
      assert(!disposed.cuts.length, 'Disposed owner cannot publish');
      results.push(
        'PASS: bounded coverage and disposal never publish a false complete overview',
      );
      for (const theme of ['dark', 'light']) {
        document.documentElement.dataset.theme = theme;
        await paint();
        capture(canvas('Timeline minimap'));
        capture(canvas('Video scrubber'));
        const before = start;
        await pointer('Timeline minimap', 'pointerdown', 450);
        await pointer('Timeline minimap', 'pointermove', 480);
        await pointer('Timeline minimap', 'pointerup', 480);
        assert(
          start > before && !active,
          'Minimap drag must navigate without retaining capture',
        );
        start = 10;
        await paint();
        await pointer('Video scrubber', 'pointerdown', 150);
        await pointer('Video scrubber', 'pointermove', 300);
        assert(
          !seeks.length && active,
          'Scrubber must preview without queuing media seeks',
        );
        await pointer('Video scrubber', 'pointerup', 450);
        assert(
          seeks.at(-1) === 25 && !active,
          'Scrubber commits final position once',
        );
        seeks = [];
        const pixels = canvas('Timeline minimap')
          .getContext('2d')!
          .getImageData(0, 0, 600, 20).data;
        assert(
          new Set(pixels).size > 5,
          'Minimap must paint cue density/cut/viewport marks',
        );
        results.push(
          `PASS: ${theme} minimap navigation and same-window scrubber final seek`,
        );
      }
      for (const reason of [
        'replacement',
        'viewport',
        'disabled',
        'resize',
        'Escape',
        'blur',
        'unmount',
      ]) {
        enabled = true;
        mounted = true;
        width = 600;
        await paint();
        capture(canvas('Video scrubber'));
        await pointer('Video scrubber', 'pointerdown', 150);
        await pointer('Video scrubber', 'pointermove', 300);
        if (reason === 'replacement') session++;
        else if (reason === 'viewport') start += 1;
        else if (reason === 'disabled') enabled = false;
        else if (reason === 'resize') width = 550;
        else if (reason === 'unmount') mounted = false;
        else if (reason === 'blur') window.dispatchEvent(new Event('blur'));
        else
          canvas('Video scrubber').dispatchEvent(
            new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
          );
        await paint();
        if (mounted) await pointer('Video scrubber', 'pointerup', 450);
        assert(
          !seeks.length && !active,
          `${reason} must cancel without a later seek`,
        );
        results.push(`PASS: scrubber ${reason} cancels transient seek`);
      }
      mounted = true;
      enabled = true;
      await paint();
      canvas('Timeline minimap').dispatchEvent(
        new KeyboardEvent('keydown', { key: 'End', bubbles: true }),
      );
      await tick();
      assert(start === 80, 'Keyboard End must clamp viewport');
      results.push(
        'PASS: minimap keyboard navigation clamps to full-file extent',
      );
      complete = true;
    } catch (error) {
      results.push(`FAIL: ${String(error)}`);
    }
  }
  onMount(() => {
    void run();
  });
</script>

<h1>Timeline overview and scrubber</h1>
<p>
  Production controls with modeled scene reads and generated pointer events; no
  native playback claim.
</p>
<div bind:this={host} style:width={`${width}px`}>
  {#if mounted}
    <TimelineStrip
      overview
      {cues}
      issues={new Map([['500', 'violation']])}
      cuts={[12, 19]}
      {start}
      {span}
      duration={100}
      position={14}
      selected={500}
      {enabled}
      mediaKey={session}
      onmove={(value) => (start = value)}
      ongesture={(value) => (active = value)}
    />
    <TimelineStrip
      {cues}
      cuts={[12, 19]}
      {start}
      {span}
      duration={100}
      position={14}
      selected={500}
      {enabled}
      mediaKey={session}
      onmove={(value) => seeks.push(value)}
      ongesture={(value) => (active = value)}
    />
  {/if}
</div>
<ul>
  {#each results as result (result)}<li>{result}</li>{/each}
</ul>
{#if complete}<strong>PASS: all {results.length} overview checks</strong>{/if}
