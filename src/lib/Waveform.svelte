<script lang="ts">
  import {
    defaultTiming,
    snapBoundary,
    type TimingSettings,
  } from './magnetic-timing';
  import { onMount } from 'svelte';
  import { wheelViewport } from './waveform-navigation';
  import type { Cue, WaveformView } from './client';
  import { clamp, orderedRange, timeToX, xToTime } from './timeline';
  let {
    view,
    start,
    span,
    position,
    duration,
    cues,
    range,
    enabled = true,
    onseek,
    onrange,
    onwidth,
    onnavigate,
    selected = null,
    previewRange = null,
    oncontext,
    onselect,
    ongesture,
    sceneCuts = [],
    timing = defaultTiming,
    editingCueId = null,
  }: {
    sceneCuts?: number[];
    timing?: TimingSettings;
    editingCueId?: number | null;
    view: WaveformView | null;
    start: number;
    span: number;
    position: number | null;
    duration: number;
    cues: Cue[];
    range: [number, number] | null;
    enabled?: boolean;
    onseek: (time: number) => void;
    onselect?: (cueId: number) => void;
    ongesture?: (active: boolean) => void;
    onrange: (range: [number, number]) => void;
    onwidth: (width: number) => void;
    selected?: number | null;
    previewRange?: [number, number] | null;
    oncontext?: (
      x: number,
      y: number,
      cueId: number | null,
      inRange: boolean,
    ) => void;
    onnavigate?: (view: { start: number; span: number }) => void;
  } = $props();
  let canvas: HTMLCanvasElement;
  let width = $state(600);
  let height = $state(160);
  let gesture = $state.raw<{
    pointer: number;
    cues: Cue[];
    cuts: number[];
    timing: TimingSettings;
    anchorX: number;
    cueId: number | null;
    anchor: number;
    left: number;
    width: number;
    start: number;
    span: number;
    duration: number;
    preview: [number, number] | null;
  } | null>(null);
  const displayedRange = $derived(gesture?.preview ?? previewRange ?? range);
  function draw() {
    if (!canvas) return;
    const context = canvas.getContext('2d');
    if (!context) return;
    const scale = devicePixelRatio || 1;
    canvas.width = Math.round(width * scale);
    canvas.height = Math.round(height * scale);
    context.scale(scale, scale);
    context.fillStyle = '#101c25';
    context.fillRect(0, 0, width, height);
    const x = (t: number) => timeToX(t, start, span, width);
    if (timing.enabled) {
      const band = (from: number, to: number, color: string) => {
        const left = Math.max(0, x(from));
        const right = Math.min(width, x(to));
        if (right <= left) return;
        context.fillStyle = color;
        context.fillRect(left, 20, right - left, height - 20);
      };
      if (timing.snapCues)
        for (const cue of cues) {
          // The moving cue is not its own magnetic neighbor.
          if (cue.id === editingCueId) continue;
          band(
            (cue.startMs - timing.cueGapMs) / 1000,
            cue.startMs / 1000,
            '#75cbd626',
          );
          band(
            cue.endMs / 1000,
            (cue.endMs + timing.cueGapMs) / 1000,
            '#75cbd626',
          );
        }
      if (timing.snapScenes)
        for (const cut of sceneCuts)
          band(
            cut - timing.sceneBeforeMs / 1000,
            cut + timing.sceneAfterMs / 1000,
            '#e992ef24',
          );
    }
    for (const cue of cues) {
      const left = Math.max(0, x(cue.startMs / 1000));
      const right = Math.min(width, x(cue.endMs / 1000));
      if (right <= left) continue;
      context.fillStyle = cue.id === selected ? '#6b9eff40' : '#7392ca22';
      context.fillRect(left, 20, right - left, height - 20);
      context.strokeStyle = cue.id === selected ? '#a1c9ff' : '#6f90ba';
      context.lineWidth = cue.id === selected ? 2 : 1;
      context.strokeRect(
        left + 0.5,
        21,
        Math.max(0, right - left - 1),
        height - 22,
      );
      context.save();
      context.beginPath();
      context.rect(left + 4, height - 22, Math.max(0, right - left - 8), 20);
      context.clip();
      context.fillStyle = '#d6e5ff';
      context.font = '11px system-ui';
      context.fillText(
        cue.text.replaceAll('\n', ' ↵ ') || 'Untitled cue',
        left + 5,
        height - 7,
      );
      context.restore();
    }
    context.font = '10px system-ui';
    context.fillStyle = '#92a8b7';
    context.strokeStyle = '#2a3a46';
    const tick =
      Math.pow(10, Math.floor(Math.log10(span / 5))) *
      ([1, 2, 5, 10].find(
        (n) => n * Math.pow(10, Math.floor(Math.log10(span / 5))) >= span / 7,
      ) ?? 10);
    for (let t = Math.ceil(start / tick) * tick; t <= start + span; t += tick) {
      const px = x(t);
      context.beginPath();
      context.moveTo(px, 20);
      context.lineTo(px, height);
      context.stroke();
      context.fillText(`${t.toFixed(span < 5 ? 2 : 1)} s`, px + 3, 13);
    }
    const center = (height + 8) / 2;
    const amplitude = Math.max(10, (height - 58) / 2);
    context.strokeStyle = '#77d8c0';
    context.lineWidth = 1;
    context.beginPath();
    if (view)
      for (const [i, peak] of view.peaks.entries()) {
        const px = x(view.startSeconds + i * view.secondsPerPeak);
        const next = x(view.startSeconds + (i + 1) * view.secondsPerPeak);
        context.moveTo(px, center - peak[1] * amplitude);
        context.lineTo(px, center - peak[0] * amplitude);
        if (next - px > 1) {
          context.lineTo(next, center - peak[0] * amplitude);
          context.lineTo(next, center - peak[1] * amplitude);
        }
      }
    context.stroke();
    if (displayedRange) {
      context.fillStyle = '#e9ba4538';
      context.fillRect(
        x(displayedRange[0]),
        20,
        x(displayedRange[1]) - x(displayedRange[0]),
        height - 20,
      );
      context.strokeStyle = '#e9ba45';
      for (const t of displayedRange) {
        context.beginPath();
        context.moveTo(x(t), 20);
        context.lineTo(x(t), height);
        context.stroke();
      }
    }
    // Cut candidates are an overlay, not editable subtitle boundaries.
    context.strokeStyle = '#e992ef';
    context.lineWidth = 1;
    context.setLineDash([3, 3]);
    for (const cut of sceneCuts) {
      if (cut < start || cut > start + span) continue;
      const px = x(cut);
      context.beginPath();
      context.moveTo(px, 20);
      context.lineTo(px, height);
      context.stroke();
    }
    context.setLineDash([]);
    if (position === null) return;
    context.strokeStyle = '#ff7f7f';
    context.lineWidth = 2;
    context.beginPath();
    context.moveTo(x(position), 18);
    context.lineTo(x(position), height);
    context.stroke();
  }
  $effect(() => {
    void view;
    void start;
    void span;
    void position;
    void cues;
    void selected;
    void sceneCuts;
    void timing;
    void editingCueId;
    void displayedRange;
    void width;
    void height;
    draw();
  });
  onMount(() => {
    const observer = new ResizeObserver(([entry]) => {
      if (!entry) return;
      cancel();
      width = entry.contentRect.width;
      height = entry.contentRect.height;
      onwidth(width);
    });
    observer.observe(canvas);
    canvas.addEventListener('wheel', navigate, { passive: false });
    return () => {
      canvas.removeEventListener('wheel', navigate);
      observer.disconnect();
      cancel();
    };
  });
  $effect(() => {
    if (
      gesture &&
      (!enabled ||
        gesture.start !== start ||
        gesture.span !== span ||
        gesture.duration !== duration)
    )
      cancel();
  });
  function navigate(event: WheelEvent) {
    if (!enabled || !onnavigate) return;
    const rect = canvas.getBoundingClientRect();
    const next = wheelViewport(start, span, duration, rect.width, {
      deltaX: event.deltaX,
      deltaY: event.deltaY,
      deltaMode: event.deltaMode,
      shiftKey: event.shiftKey,
      offsetX: event.clientX - rect.left,
    });
    if (!next) return;
    event.preventDefault();
    cancel();
    onnavigate(next);
  }
  function cancel() {
    const previous = gesture;
    gesture = null;
    if (previous) ongesture?.(false);
    if (previous && canvas.hasPointerCapture(previous.pointer))
      canvas.releasePointerCapture(previous.pointer);
  }
  function time(
    e: PointerEvent,
    mapping: {
      left: number;
      start: number;
      span: number;
      width: number;
      duration: number;
    },
  ): number {
    return clamp(
      xToTime(
        e.clientX - mapping.left,
        mapping.start,
        mapping.span,
        mapping.width,
      ),
      0,
      mapping.duration,
    );
  }
  function begin(e: PointerEvent) {
    if (gesture || !enabled || !e.isPrimary || e.button !== 0 || duration <= 0)
      return;
    const rect = canvas.getBoundingClientRect();
    if (rect.width <= 0) return;
    e.preventDefault();
    const mapping = {
      left: rect.left,
      width: rect.width,
      start,
      span,
      duration,
    };
    canvas.focus({ preventScroll: true });
    canvas.setPointerCapture(e.pointerId);
    gesture = {
      ...mapping,
      pointer: e.pointerId,
      cues,
      cuts: [...sceneCuts],
      timing: { ...timing },
      anchor: time(e, mapping),
      anchorX: e.clientX,
      cueId: e.clientY - rect.top >= 20 ? cueAt(time(e, mapping)) : null,
      preview: null,
    };
    ongesture?.(true);
  }
  function snappedRange(
    e: PointerEvent,
    mapping: NonNullable<typeof gesture>,
  ): [number, number] {
    const raw = orderedRange(
      mapping.anchor,
      time(e, mapping),
      mapping.duration,
    );
    const tolerance =
      ((mapping.span * 1000) / mapping.width) * mapping.timing.snapDistancePx;
    const first = snapBoundary(
      Math.round(raw[0] * 1000),
      'start',
      Math.round(raw[1] * 1000),
      null,
      mapping.cues,
      mapping.cuts,
      mapping.timing,
      tolerance,
      Math.floor(mapping.duration * 1000),
      e.altKey,
    );
    const last = snapBoundary(
      Math.round(raw[1] * 1000),
      'end',
      first.milliseconds,
      null,
      mapping.cues,
      mapping.cuts,
      mapping.timing,
      tolerance,
      Math.floor(mapping.duration * 1000),
      e.altKey,
    );
    // Preserve the original fractional pointer mapping when no target is selected.
    return [
      first.target ? first.milliseconds / 1000 : raw[0],
      last.target ? last.milliseconds / 1000 : raw[1],
    ];
  }
  function move(e: PointerEvent) {
    if (!gesture || gesture.pointer !== e.pointerId) return;
    gesture = {
      ...gesture,
      preview:
        Math.abs(e.clientX - gesture.anchorX) > 3
          ? snappedRange(e, gesture)
          : null,
    };
  }
  function end(e: PointerEvent) {
    const previous = gesture;
    if (!previous || previous.pointer !== e.pointerId) return;
    // Clear capture ownership before publishing so capture loss cannot cancel
    // another gesture. Until release, the selected playback range is unchanged.
    cancel();
    if (Math.abs(e.clientX - previous.anchorX) <= 3) {
      const clickedCue = cues.find((cue) => cue.id === previous.cueId);
      const alreadyRanged =
        clickedCue &&
        selected === clickedCue.id &&
        range?.[0] === clickedCue.startMs / 1000 &&
        range?.[1] === clickedCue.endMs / 1000;
      if (previous.cueId !== null && onselect && !alreadyRanged)
        onselect(previous.cueId);
      else onseek(time(e, previous));
    } else onrange(snappedRange(e, previous));
  }
  function cueAt(seconds: number): number | null {
    const matches = cues.filter(
      (c) => c.startMs <= seconds * 1000 && seconds * 1000 < c.endMs,
    );
    return (
      (matches.find((c) => c.id === selected) ?? matches.at(-1))?.id ?? null
    );
  }
  function contextMenu(e: MouseEvent) {
    if (!enabled || !oncontext) return;
    e.preventDefault();
    cancel();
    const rect = canvas.getBoundingClientRect();
    const t = xToTime(e.clientX - rect.left, start, span, rect.width);
    oncontext(
      e.clientX,
      e.clientY,
      e.clientY - rect.top >= 20 ? cueAt(t) : null,
      !!range && t >= range[0] && t <= range[1],
    );
  }
  function cancelPointer(e: PointerEvent) {
    if (gesture?.pointer === e.pointerId) cancel();
  }
</script>

<svelte:window
  onblur={cancel}
  onkeydown={(e) => {
    if (e.key === 'Escape' && gesture) {
      e.preventDefault();
      cancel();
    }
  }}
/>
<canvas
  bind:this={canvas}
  aria-label="Audio waveform: click a cue to select it, click empty space to seek, drag to select a range"
  title="Click a cue to select its range; click inside it again to seek. Scroll horizontally to pan; vertically to zoom. Shift+wheel also pans."
  tabindex="0"
  oncontextmenu={contextMenu}
  onpointerdown={begin}
  onpointermove={move}
  onpointerup={end}
  onpointercancel={cancelPointer}
  onlostpointercapture={cancelPointer}
></canvas>

<style>
  canvas {
    display: block;
    width: 100%;
    height: 100%;
    min-height: 0;
    touch-action: none;
    cursor: crosshair;
  }
</style>
