<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Waveform from '../../../src/lib/timeline/Waveform.svelte';
  import { defaultTiming } from '../../../src/lib/timing/magnetic-timing';
  import type { Cue } from '../../../src/lib/client';

  let sceneCuts = $state<number[]>([]);
  let timing = $state({ ...defaultTiming });
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
  let position = $state<number | null>(null);
  let issues = $state.raw<ReadonlyMap<string, 'violation'>>(new Map());
  let selected = $state<number | null>(null);
  let selections: number[] = [];
  let activeGesture = false;
  let testEdges = $state(false);
  const edges: {
    id: number;
    boundary?: string;
    ms?: number;
    commit?: boolean;
  }[] = [];

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
    sceneCuts = [];
    timing = { ...defaultTiming };
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
    position = null;
    issues = new Map();
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
  async function checkCueLayer() {
    await reset();
    range = null;
    cues = [
      { id: 1, startMs: 1000, endMs: 2000, text: 'Sample one' },
      { id: 2, startMs: 2500, endMs: 3500, text: 'Sample two' },
    ];
    selected = 1;
    sceneCuts = [2.1];
    const draws: string[] = [];
    const original = CanvasRenderingContext2D.prototype.fillText;
    CanvasRenderingContext2D.prototype.fillText = function (label, ...args) {
      if (label.startsWith('Sample ')) draws.push(label);
      original.call(this, label, ...args);
    };
    try {
      for (const zoom of [4, 10, 25]) {
        span = zoom;
        await paint();
        const baseline = canvas().toDataURL();
        assert(draws.includes('Sample one'), 'Zoom must draw selected label');
        draws.length = 0;
        position = 3;
        await paint();
        assert(canvas().toDataURL() !== baseline, 'Playhead must move');
        position = null;
        await paint();
        assert(
          canvas().toDataURL() === baseline,
          'Restoring playhead must restore every pixel',
        );
        assert(
          draws.length === 0,
          'Moving only the playhead must reuse cue pixels',
        );
      }
      const element = canvas();
      const context = element.getContext('2d')!;
      const copy = context.drawImage.bind(context);
      let presentations = 0;
      const previousWidth = element.width;
      const previousHeight = element.height;
      try {
        // A dimension write clears the canvas even when assigned the same value.
        for (const dimension of ['width', 'height'] as const) {
          Object.defineProperty(element, dimension, {
            configurable: true,
            get: () => (dimension === 'width' ? previousWidth : previousHeight),
            set: () => {
              throw new Error('Resized a fixed viewport during playback');
            },
          });
        }
        context.drawImage = (...args: unknown[]) => {
          presentations++;
          Reflect.apply(copy, context, args);
        };
        position = 0.5;
        await tick();
        position = 1.5;
        await tick();
        position = 2.5;
        await paint();
        assert(
          presentations === 1,
          'Multiple playhead updates before a frame must paint once',
        );
      } finally {
        context.drawImage = copy;
        Reflect.deleteProperty(element, 'width');
        Reflect.deleteProperty(element, 'height');
      }
      position = null;
      draws.length = 0;
      cues = cues.map((cue) => ({ ...cue }));
      await paint();
      assert(
        draws.length === 0,
        'Equivalent cue snapshots must reuse the layer',
      );
      cues = Array.from({ length: 512 }, (_, i) => ({
        id: i + 1,
        startMs: 1010,
        endMs: 1011,
        text: 'Sample ' + (i + 1),
      }));
      selected = 100;
      draws.length = 0;
      await paint();
      assert(
        draws.length === 1 && draws[0] === 'Sample 100',
        'Only the selected label draws when every clip is empty',
      );
      // Coincident cues exercise the original paint order. The last cue's
      // issue stripe and the selected label still draw when their bounds fit.
      cues = cues.map((cue) => ({ ...cue, endMs: 5010 }));
      selected = 512;
      issues = new Map([['512', 'violation']]);
      draws.length = 0;
      await paint();
      assert(draws.includes('Sample 512'), 'Dense selected cue lost its label');
      const x = Math.ceil((canvas().width * 1.01) / span);
      const stripe = () =>
        context
          .getImageData(x + 1, Math.round((40 * canvas().width) / width), 1, 1)
          .data.join(',');
      const marked = stripe();
      const markedImage = canvas().toDataURL();
      issues = new Map();
      await paint();
      assert(
        marked !== stripe() && markedImage !== canvas().toDataURL(),
        'Dense issue stripe must update',
      );
      // The last backing pixel may be only partly covered by each of the two
      // background fills (waveform, then gap strip). Preserve both blends.
      cues = [];
      sceneCuts = [];
      width = 600 + 0.65 / devicePixelRatio;
      await paint();
      const surface = canvas();
      const bounds = surface.getBoundingClientRect();
      const reference = document.createElement('canvas');
      reference.width = surface.width;
      reference.height = surface.height;
      const expected = reference.getContext('2d')!;
      expected.scale(devicePixelRatio, devicePixelRatio);
      expected.fillRect(0, 0, bounds.width, bounds.height);
      expected.fillRect(0, bounds.height - 12, bounds.width, 12);
      const edgeAlpha = (ctx: CanvasRenderingContext2D) =>
        ctx.getImageData(surface.width - 1, surface.height - 2, 1, 1).data[3];
      assert(
        edgeAlpha(surface.getContext('2d')!) === edgeAlpha(expected),
        'Fractional gap-strip edge lost a background blend',
      );
    } finally {
      CanvasRenderingContext2D.prototype.fillText = original;
    }
  }

  async function run() {
    try {
      await reset();
      cues = [{ id: 1, startMs: 1000, endMs: 2000, text: 'Hover' }];
      await tick();
      await pointer('pointerenter', 90);
      const cursor = () => getComputedStyle(canvas()).cursor;
      assert(
        cursor() === 'pointer',
        'Entering a selectable cue should show a hand',
      );
      selected = 1;
      await tick();
      assert(
        cursor() === 'crosshair',
        'Selection updates under a stationary pointer',
      );
      selected = null;
      start = 2;
      await tick();
      assert(cursor() === 'crosshair', 'Panning away should clear the hand');
      start = 0;
      await tick();
      assert(cursor() === 'pointer', 'Panning back should restore the hand');
      await pointer('pointerleave', 90);
      assert(cursor() === 'crosshair', 'Leaving clears the hover position');
      await checkCueLayer();
      results.push(
        'PASS: waveform layers preserve zoomed cue pixels and reuse them while the playhead moves',
      );
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
          range === null &&
          seeks.length === 1 &&
          Math.abs(seeks[0]! - 181 / 60) < 0.00001,
        'Empty-space click must clear the range and seek',
      );
      results.push(
        'PASS: empty-space click clears the selected range and seeks',
      );
      await reset();
      canvas().focus();
      canvas().dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'Escape',
          bubbles: true,
          cancelable: true,
        }),
      );
      assert(
        range === null && !seeks.length,
        'Escape on the waveform clears the range without seeking',
      );
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
      for (const theme of ['dark', 'light']) {
        await reset();
        document.documentElement.dataset.theme = theme;
        range = null;
        sceneCuts = [5];
        await paint();
        const element = canvas(),
          ctx = element.getContext('2d')!;
        const scale = element.width / element.getBoundingClientRect().width;
        const pixel = (x: number, y: number) =>
          Array.from(
            ctx.getImageData(Math.floor(x * scale), Math.floor(y * scale), 1, 1)
              .data,
          ).join(',');
        const cut = pixel(300, 40);
        assert(
          Array.from({ length: 30 }, (_, i) => pixel(300, 40 + i)).every(
            (value) => value === cut,
          ),
          'Cut must be solid, not dashed',
        );
        assert(cut !== pixel(303, 40), 'Cut must contrast with background');
        cues = [
          { id: 1, startMs: 1000, endMs: 2000, text: '' },
          { id: 2, startMs: 3000, endMs: 4000, text: '' },
        ];
        await paint();
        assert(
          pixel(150, 149) !== pixel(270, 149),
          'Gap lane must paint neutral gaps',
        );
        timing = { ...defaultTiming, enabled: true, cueGapMs: 1500 };
        await paint();
        assert(
          pixel(150, 149) !== pixel(270, 149),
          'Gap lane must paint below-minimum warning',
        );
        sceneCuts = [];
        cues = [{ id: 500, startMs: 5000, endMs: 6000, text: '' }];
        timing = { ...defaultTiming, enabled: true };
        await paint();
        await pointer('pointerdown', 60);
        await pointer('pointermove', 294);
        assert(
          !!host
            .querySelector('[role="status"]')
            ?.textContent?.includes('80 ms before #12'),
          'Snap label must name original cue number and legal endpoint',
        );
        canvas().dispatchEvent(
          new PointerEvent('pointermove', {
            pointerId: 1,
            isPrimary: true,
            button: 0,
            bubbles: true,
            altKey: true,
            clientX: canvas().getBoundingClientRect().left + 294,
          }),
        );
        await tick();
        assert(
          !canvas().getAttribute('aria-describedby'),
          'Alt bypass clears snap label',
        );
        window.dispatchEvent(new Event('blur'));
        await paint();
        results.push(
          `PASS: ${theme} solid cuts, gap lane, named snap guide and Alt bypass`,
        );
      }
      await reset();
      testEdges = true;
      cues = [{ id: 5, startMs: 2000, endMs: 4000, text: 'Unselected' }];
      await paint();
      await pointer('pointerdown', 123);
      await pointer('pointermove', 126);
      assert(!edges.length, 'Three pixels of movement remain a click');
      await pointer('pointerup', 126);
      assert(
        selections.at(-1) === 5 && !edges.length,
        'Clicking near an unselected edge selects normally without timing edits',
      );
      selected = 5;
      range = [2, 4];
      await tick();
      await pointer('pointerdown', 120);
      await pointer('pointerup', 120);
      assert(
        seeks.at(-1) === 2 && !edges.length,
        'Clicking a selected start edge seeks without timing edits',
      );
      await pointer('pointerdown', 240);
      await pointer('pointerup', 240);
      assert(
        seeks.at(-1) === 4 && !edges.length,
        'Clicking a cue end boundary seeks without timing edits',
      );
      results.push(
        'PASS: cue-edge clicks and three-pixel movement preserve ordinary selection and seeking',
      );
      await pointer('pointerdown', 123);
      await paint();
      await pointer('pointermove', 183);
      await pointer('pointerup', 183);
      assert(
        edges[0]?.id === 5 &&
          edges.at(-1)?.commit === true &&
          edges.at(-2)?.ms === 3000 &&
          !ranges.length,
        'Nearby edge starts boundary gesture and commits once',
      );
      await pointer('pointerdown', 130);
      await pointer('pointermove', 190);
      await pointer('pointerup', 190);
      assert(
        ranges.length === 1,
        'Near miss outside six pixels remains range selection',
      );
      await pointer('pointerdown', 120);
      await pointer('pointermove', 130);
      await paint();
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      await tick();
      assert(edges.at(-1)?.commit === false, 'Escape cancels an edge gesture');
      results.push(
        'PASS: unselected edges, six-pixel near misses and Escape use boundary callbacks',
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
      {sceneCuts}
      {timing}
      cueNumbers={new Map([[500, 12]])}
      {start}
      {span}
      {duration}
      {enabled}
      {position}
      {cues}
      {selected}
      {issues}
      onedge={testEdges
        ? {
            begin: async (id, current) => {
              if (!current()) return false;
              edges.push({ id });
              return true;
            },
            preview: (boundary, ms) => edges.push({ id: 5, boundary, ms }),
            finish: (commit) => edges.push({ id: 5, commit }),
          }
        : undefined}
      onselect={(id) => selections.push(id)}
      ongesture={(active) => (activeGesture = active)}
      {range}
      onseek={(value) => seeks.push(value)}
      onclearrange={(seconds) => {
        range = null;
        if (seconds !== undefined) seeks.push(seconds);
      }}
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
