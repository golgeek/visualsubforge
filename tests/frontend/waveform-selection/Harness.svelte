<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Waveform from '../../../src/lib/Waveform.svelte';
  import type { Cue } from '../../../src/lib/client';

  let host: HTMLDivElement;
  let mounted = $state(true);
  let width = $state(600);
  let start = $state(0);
  let span = $state(10);
  let duration = $state(60);
  let enabled = $state(true);
  let range = $state<[number, number] | null>([1, 2]);
  let results = $state<string[]>([]);
  let complete = $state(false);
  let ranges: [number, number][] = [];
  let seeks: number[] = [];
  let cues = $state<Cue[]>([]);
  let selected = $state<number | null>(null);
  let selections: number[] = [];
  let activeGesture = false;
  function assert(condition: boolean, message: string) {
    if (!condition) throw new Error(message);
  }
  function canvas(): HTMLCanvasElement {
    const element = host.querySelector('canvas');
    if (!element) throw new Error('Missing waveform canvas');
    return element;
  }
  async function paint() {
    await tick();
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    );
  }
  async function reset() {
    mounted = false;
    await tick();
    ranges = [];
    seeks = [];
    cues = [];
    selected = null;
    selections = [];
    activeGesture = false;
    range = [1, 2];
    start = 0;
    span = 10;
    duration = 60;
    enabled = true;
    width = 600;
    mounted = true;
    await paint();
    // Handler/lifecycle coverage only. Synthetic events cannot acquire OS capture.
    const element = canvas();
    let captured: number | null = null;
    element.setPointerCapture = (id) => {
      captured = id;
    };
    element.hasPointerCapture = (id) => captured === id;
    element.releasePointerCapture = () => {
      captured = null;
    };
  }
  async function pointer(type: string, x: number, pointerId = 1, button = 0) {
    const element = canvas();
    element.dispatchEvent(
      new PointerEvent(type, {
        pointerId,
        button,
        isPrimary: pointerId === 1,
        bubbles: true,
        clientX: element.getBoundingClientRect().left + x,
        clientY: element.getBoundingClientRect().top + 40,
      }),
    );
    await tick();
  }
  async function run() {
    try {
      await reset();
      await pointer('pointerdown', 180, 1, 2);
      await pointer('pointerup', 180, 1, 2);
      assert(
        !ranges.length && !seeks.length,
        'Right-click must not seek or select',
      );
      results.push('PASS: right-click leaves selection and position unchanged');

      await reset();
      await pointer('pointerdown', 180);
      await pointer('pointermove', 300);
      assert(
        !ranges.length && range?.[0] === 1 && range?.[1] === 2,
        'Drag preview must not publish an accepted range',
      );
      await pointer('pointerup', 360);
      await pointer('pointerup', 360);
      assert(
        ranges.length === 1 && ranges[0]?.[0] === 3 && ranges[0]?.[1] === 6,
        'Release must include final coordinates and publish exactly once',
      );
      results.push('PASS: range preview commits final coordinates once');

      for (const cancellation of [
        'Escape',
        'window blur',
        'pointercancel',
        'lostpointercapture',
        'resize',
        'viewport',
        'zoom',
        'duration',
        'disabled',
        'unmount',
      ]) {
        await reset();
        await pointer('pointerdown', 180);
        await pointer('pointermove', 300);
        if (cancellation === 'Escape')
          window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
        else if (cancellation === 'window blur')
          window.dispatchEvent(new Event('blur'));
        else if (cancellation === 'resize') width = 500;
        else if (cancellation === 'viewport') start = 10;
        else if (cancellation === 'zoom') span = 5;
        else if (cancellation === 'duration') duration = 30;
        else if (cancellation === 'disabled') enabled = false;
        else if (cancellation === 'unmount') mounted = false;
        else await pointer(cancellation, 300);
        await paint();
        if (mounted) {
          await pointer('pointermove', 360);
          await pointer('pointerup', 360);
        }
        assert(
          !ranges.length &&
            !seeks.length &&
            range?.[0] === 1 &&
            range?.[1] === 2,
          `${cancellation}: cancelled drag must preserve the original range`,
        );
        results.push(`PASS: ${cancellation} cancels without a later commit`);
      }

      await reset();
      await pointer('pointerdown', 180);
      await pointer('pointermove', 300, 2);
      await pointer('pointercancel', 300, 2);
      await pointer('pointerup', 300, 2);
      assert(
        !ranges.length && !seeks.length,
        'Foreign pointer must not control the gesture',
      );
      await pointer('pointerup', 360);
      assert(
        ranges.length === 1 && ranges[0]?.[0] === 3 && ranges[0]?.[1] === 6,
        'Original pointer must still commit after unrelated input',
      );
      results.push(
        'PASS: unrelated pointer cannot move, cancel or commit selection',
      );

      await reset();
      await pointer('pointerdown', 180);
      await pointer('pointerup', 181);
      assert(
        !ranges.length &&
          seeks.length === 1 &&
          Math.abs(seeks[0]! - 181 / 60) < 0.00001,
        'Ordinary click must seek without replacing range',
      );
      results.push('PASS: click seeks and preserves the selected range');
      await reset();
      enabled = false;
      await tick();
      await pointer('pointerdown', 180);
      await pointer('pointerup', 360);
      assert(
        !ranges.length && !seeks.length,
        'Disabled waveform rejects new selection and seek',
      );
      results.push('PASS: disabled waveform cannot start a gesture');
      await reset();
      const wheel = (deltaX: number, deltaY: number) => {
        const element = canvas();
        const event = new WheelEvent('wheel', {
          deltaX,
          deltaY,
          bubbles: true,
          cancelable: true,
          clientX: element.getBoundingClientRect().left + 300,
        });
        element.dispatchEvent(event);
        return event;
      };
      await pointer('pointerdown', 180);
      await pointer('pointermove', 300);
      assert(
        wheel(60, 0).defaultPrevented,
        'Wheel must prevent page scrolling',
      );
      await tick();
      await pointer('pointerup', 360);
      assert(
        start === 1 && span === 10 && !ranges.length && !seeks.length,
        'Pan must cancel drag without changing range or seeking',
      );
      results.push(
        'PASS: wheel pans and cancels pending selection without seeking',
      );
      const anchor = start + span / 2;
      wheel(0, -100);
      await tick();
      assert(
        span < 10 &&
          Math.abs(start + span / 2 - anchor) < 1e-10 &&
          !seeks.length,
        'Zoom must preserve pointer anchor and transport',
      );
      results.push('PASS: wheel zoom preserves pointer anchor and transport');
      enabled = false;
      await tick();
      const priorStart = start;
      const priorSpan = span;
      assert(
        !wheel(60, -100).defaultPrevented,
        'Disabled navigation must not consume wheel',
      );
      await tick();
      assert(
        start === priorStart && span === priorSpan,
        'Disabled navigation changed viewport',
      );
      results.push('PASS: disabled waveform ignores wheel navigation');
      await reset();
      cues = [
        { id: 1, startMs: 2000, endMs: 4000, text: 'First' },
        { id: 2, startMs: 3000, endMs: 5000, text: 'Overlap' },
      ];
      await tick();
      await pointer('pointerdown', 150);
      assert(activeGesture, 'Pointer down must suspend viewport following');
      await pointer('pointerup', 150);
      assert(
        selections[0] === 1 &&
          !seeks.length &&
          !ranges.length &&
          !activeGesture,
        'Cue click must select without seeking/ranging and release viewport',
      );
      results.push(
        'PASS: cue click selects without seeking and brackets viewport freeze',
      );
      selected = 1;
      await tick();
      await pointer('pointerdown', 210);
      await pointer('pointerup', 210);
      assert(selections.at(-1) === 1, 'Selected overlap must win');
      selected = null;
      await tick();
      await pointer('pointerdown', 210);
      await pointer('pointerup', 210);
      assert(selections.at(-1) === 2, 'Topmost overlap must win otherwise');
      await pointer('pointerdown', 240);
      await pointer('pointerup', 240);
      assert(selections.at(-1) === 2, 'Half-open endpoint selected ending cue');
      results.push(
        'PASS: overlap selection agrees with context menu and half-open intervals',
      );
      const count = selections.length;
      await pointer('pointerdown', 150);
      await pointer('pointerup', 360);
      assert(
        selections.length === count && ranges.length === 1 && !seeks.length,
        'Drag beginning in cue must remain range selection',
      );
      await pointer('pointerdown', 150);
      window.dispatchEvent(new Event('blur'));
      await tick();
      assert(
        !activeGesture && selections.length === count,
        'Cancellation must release viewport without selection',
      );
      results.push(
        'PASS: cue drags select ranges; cancelled clicks release viewport without selecting',
      );
      selected = 1;
      range = [2, 4];
      await tick();
      const selectionCount = selections.length;
      await pointer('pointerdown', 210);
      await pointer('pointerup', 210);
      assert(
        selections.length === selectionCount &&
          seeks.at(-1) === 3.5 &&
          range[0] === 2 &&
          range[1] === 4,
        'Repeated cue click must seek inside unchanged range',
      );
      range = [2.1, 4];
      await tick();
      await pointer('pointerdown', 210);
      await pointer('pointerup', 210);
      assert(
        selections.length === selectionCount + 1 && selections.at(-1) === 1,
        'Different range must select cue again',
      );
      results.push(
        'PASS: exact selected cue range seeks on repeat click; a different range reselects',
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

<h1>Waveform selection interaction regression</h1>
<p>
  Production component; synthetic pointer/keyboard events and real layout
  lifecycle. No native capture or audio claim.
</p>
<div bind:this={host} style:width={`${width}px`} style="height: 160px;">
  {#if mounted}
    <Waveform
      view={null}
      {start}
      {span}
      {duration}
      {enabled}
      position={null}
      {cues}
      {selected}
      onselect={(id) => selections.push(id)}
      ongesture={(active) => (activeGesture = active)}
      {range}
      onseek={(value) => seeks.push(value)}
      onrange={(value) => {
        ranges.push(value);
        range = value;
      }}
      onwidth={() => {}}
      onnavigate={(next) => {
        start = next.start;
        span = next.span;
      }}
    />
  {/if}
</div>
<ul>
  {#each results as result (result)}<li>{result}</li>{/each}
</ul>
{#if complete}<strong>PASS: all {results.length} interaction checks</strong
  >{/if}
