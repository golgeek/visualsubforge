<script lang="ts">
  import { onMount } from 'svelte';
  import type { Cue } from '../client';
  import type { IssueSeverity } from '../quality/cue-issues';
  import { readCanvasPalette, watchCanvasPalette } from '../ui/canvas-palette';
  import { cueDensity } from './timeline-marks';
  import { clamp } from './timeline';
  let {
    overview = false,
    cues,
    issues = new Map(),
    cuts = [],
    start,
    span,
    duration,
    position = null,
    selected = null,
    enabled,
    mediaKey,
    incomplete = false,
    onmove,
    ongesture = () => {},
  }: {
    overview?: boolean;
    cues: readonly Cue[];
    issues?: ReadonlyMap<string, IssueSeverity>;
    cuts?: readonly number[];
    start: number;
    span: number;
    duration: number;
    position?: number | null;
    selected?: number | null;
    enabled: boolean;
    mediaKey: number;
    incomplete?: boolean;
    onmove: (seconds: number) => void;
    ongesture?: (active: boolean) => void;
  } = $props();
  let canvas: HTMLCanvasElement;
  let width = $state(600);
  let preview = $state<number | null>(null);
  const playhead = $derived(preview ?? position);
  let palette = $state.raw(
    readCanvasPalette(getComputedStyle(document.documentElement)),
  );
  const selectedCue = $derived(cues.find((cue) => cue.id === selected));
  const issueBins = $derived.by(() => {
    const bins = new Array<IssueSeverity | undefined>(1024);
    if (duration > 0)
      for (const cue of cues) {
        const severity = issues.get(String(cue.id));
        if (!severity) continue;
        const bin = Math.max(
          0,
          Math.min(1023, Math.floor((cue.startMs / 1000 / duration) * 1024)),
        );
        if (bins[bin] !== 'violation') bins[bin] = severity;
      }
    return bins;
  });
  const cutTicks = $derived(
    overview && duration > 0
      ? [
          ...new Set(cuts.map((cut) => Math.round((cut / duration) * 1024))),
        ].map((bin) => (bin / 1024) * duration)
      : cuts,
  );
  const density = $derived(cueDensity(cues, duration));
  const label = $derived(overview ? 'Timeline minimap' : 'Video scrubber');
  const maximum = $derived(
    overview ? Math.max(0, duration - span) : Math.min(duration, start + span),
  );
  const minimum = $derived(overview ? 0 : start);
  const value = $derived(
    overview ? start : clamp(playhead ?? start, minimum, maximum),
  );
  let drag = $state.raw<{
    pointer: number;
    left: number;
    width: number;
    start: number;
    span: number;
    duration: number;
    mediaKey: number;
    offset: number;
  } | null>(null);
  function draw() {
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const scale = devicePixelRatio || 1;
    canvas.width = Math.round(width * scale);
    canvas.height = 20 * scale;
    ctx.scale(scale, scale);
    const c = palette;
    ctx.fillStyle = c.background;
    ctx.fillRect(0, 0, width, 20);
    const origin = overview ? 0 : start,
      extent = overview ? duration : span;
    if (!(extent > 0)) return;
    const x = (seconds: number) => ((seconds - origin) / extent) * width;
    if (overview) {
      const maximum = Math.max(1, ...density);
      ctx.fillStyle = c.cueFill;
      for (const [i, count] of density.entries())
        if (count)
          ctx.fillRect(
            (i / density.length) * width,
            17 - (count / maximum) * 12,
            Math.max(1, width / density.length),
            (count / maximum) * 12,
          );
      for (const [bin, severity] of issueBins.entries()) {
        if (!severity) continue;
        ctx.fillStyle = severity === 'violation' ? c.error : c.warning;
        ctx.fillRect((bin / 1024) * width, 1, 2, 3);
      }
    }
    const cue = selectedCue;
    if (cue) {
      ctx.fillStyle = c.cueFillSelected;
      ctx.fillRect(
        x(cue.startMs / 1000),
        6,
        Math.max(1, x(cue.endMs / 1000) - x(cue.startMs / 1000)),
        11,
      );
    }
    ctx.strokeStyle = c.cut;
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (const cut of cutTicks) {
      const px = x(cut);
      if (px < 0 || px > width) continue;
      ctx.moveTo(px, 4);
      ctx.lineTo(px, 17);
    }
    ctx.stroke();
    if (overview) {
      ctx.fillStyle = c.rangeFill;
      ctx.fillRect(x(start), 0, (span / extent) * width, 20);
      ctx.strokeStyle = c.accent;
      ctx.strokeRect(
        x(start) + 0.5,
        0.5,
        Math.max(1, (span / extent) * width - 1),
        19,
      );
    }
    if (playhead !== null) {
      ctx.strokeStyle = c.playhead;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(x(playhead), 0);
      ctx.lineTo(x(playhead), 20);
      ctx.stroke();
    }
    if (incomplete && overview) {
      ctx.font = `9px ${c.fontFamily}`;
      ctx.fillStyle = c.warning;
      ctx.fillText('Partial cuts', 4, 12);
    }
  }
  $effect(() => {
    void palette;
    void width;
    void density;
    void cues;
    void issues;
    void cutTicks;
    void issueBins;
    void selectedCue;
    void start;
    void span;
    void duration;
    void playhead;
    void selected;
    void incomplete;
    draw();
  });
  function cancel() {
    const old = drag;
    drag = null;
    preview = null;
    if (!old) return;
    if (canvas.hasPointerCapture(old.pointer))
      canvas.releasePointerCapture(old.pointer);
    ongesture(false);
  }
  $effect(() => {
    if (
      drag &&
      (!enabled ||
        drag.mediaKey !== mediaKey ||
        drag.duration !== duration ||
        drag.span !== span ||
        (!overview && drag.start !== start))
    )
      cancel();
  });
  function move(event: PointerEvent) {
    if (!drag || event.pointerId !== drag.pointer) return;
    const seconds =
      ((event.clientX - drag.left) / drag.width) *
        (overview ? drag.duration : drag.span) +
      (overview ? 0 : drag.start);
    const next = clamp(
      seconds - drag.offset,
      overview ? 0 : drag.start,
      overview
        ? Math.max(0, drag.duration - drag.span)
        : Math.min(drag.duration, drag.start + drag.span),
    );
    if (overview) onmove(next);
    else preview = next;
  }
  function begin(event: PointerEvent) {
    if (
      !enabled ||
      duration <= 0 ||
      drag ||
      !event.isPrimary ||
      event.button !== 0
    )
      return;
    const rect = canvas.getBoundingClientRect();
    if (rect.width <= 0) return;
    event.preventDefault();
    canvas.focus({ preventScroll: true });
    canvas.setPointerCapture(event.pointerId);
    const seconds = ((event.clientX - rect.left) / rect.width) * duration;
    drag = {
      pointer: event.pointerId,
      left: rect.left,
      width: rect.width,
      start,
      span,
      duration,
      mediaKey,
      offset: overview
        ? seconds >= start && seconds <= start + span
          ? seconds - start
          : span / 2
        : 0,
    };
    ongesture(true);
    move(event);
  }
  function end(event: PointerEvent) {
    if (drag?.pointer !== event.pointerId) return;
    move(event);
    const next = preview;
    cancel();
    if (!overview && next !== null && enabled) onmove(next);
  }
  function key(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      cancel();
      return;
    }
    if (
      !enabled ||
      event.isComposing ||
      event.ctrlKey ||
      event.metaKey ||
      event.altKey
    )
      return;
    let next: number;
    if (event.key === 'Home') next = minimum;
    else if (event.key === 'End') next = maximum;
    else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight')
      next =
        value +
        ((event.key === 'ArrowLeft' ? -1 : 1) * span) /
          (event.shiftKey ? 1 : 10);
    else return;
    event.preventDefault();
    event.stopPropagation();
    onmove(clamp(next, minimum, maximum));
  }
  onMount(() => {
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      cancel();
      width = entry.contentRect.width;
    });
    observer.observe(canvas);
    const unwatch = watchCanvasPalette(
      document.documentElement,
      () =>
        (palette = readCanvasPalette(
          getComputedStyle(document.documentElement),
        )),
    );
    return () => {
      cancel();
      observer.disconnect();
      unwatch();
    };
  });
</script>

<svelte:window onblur={cancel} />
<canvas
  bind:this={canvas}
  role="slider"
  tabindex={enabled ? 0 : -1}
  aria-label={label}
  aria-valuemin={minimum}
  aria-valuemax={maximum}
  aria-valuenow={value}
  aria-valuetext={`${value.toFixed(2)} seconds${incomplete && overview ? '; incomplete shot-change coverage' : ''}`}
  aria-disabled={!enabled}
  onpointerdown={begin}
  onpointermove={move}
  onpointerup={end}
  onpointercancel={(event) => {
    if (drag?.pointer === event.pointerId) cancel();
  }}
  onlostpointercapture={(event) => {
    if (drag?.pointer === event.pointerId) cancel();
  }}
  onkeydown={key}
></canvas>

<style>
  canvas {
    display: block;
    width: 100%;
    height: 20px;
    min-width: 0;
    flex-shrink: 0;
    touch-action: none;
    cursor: pointer;
  }
  canvas:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  canvas[aria-disabled='true'] {
    cursor: default;
  }
</style>
