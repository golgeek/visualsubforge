<script lang="ts">
  import {
    defaultTiming,
    snapBoundary,
    snapGuide,
    type SnapGuide,
    type TimingSettings,
  } from '../timing/magnetic-timing';
  import { bufferVisibility, gapMarks, type GapMark } from './timeline-marks';
  import {
    hatch,
    drawTimelineMarks,
    drawGuides,
    drawGaps,
  } from './draw-timeline-marks';
  import type { IssueSeverity } from '../quality/cue-issues';
  import { onMount } from 'svelte';
  import { devicePixelRatio } from 'svelte/reactivity/window';
  import '../ui/tokens.css';
  import {
    readCanvasPalette,
    watchCanvasPalette,
    type CanvasPalette,
  } from '../ui/canvas-palette';
  import { cueAt, exactRange, edgeAt, waveformCursor } from './waveform-hit';
  import { wheelViewport } from './waveform-navigation';
  import type { Cue, WaveformView } from '../client';
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
    onclearrange,
    onrange,
    onwidth,
    onnavigate,
    selected = null,
    previewRange = null,
    oncontext,
    onselect,
    onedge,
    ongesture,
    sceneCuts = [],
    timing = defaultTiming,
    editingCueId = null,
    externalGuide = null,
    cueNumbers = new Map(),
    issues = new Map(),
    gaps,
    warningTiming,
  }: {
    warningTiming?: TimingSettings;
    externalGuide?: SnapGuide | null;
    cueNumbers?: ReadonlyMap<number, number>;
    issues?: ReadonlyMap<string, IssueSeverity>;
    gaps?: readonly GapMark[];
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
    onclearrange?: (seek?: number) => void;
    onselect?: (cueId: number) => void;
    onedge?:
      | {
          begin: (id: number, current: () => boolean) => Promise<boolean>;
          preview: (
            boundary: 'start' | 'end',
            milliseconds: number,
            pointer: { bypass: boolean; millisecondsPerPixel: number },
          ) => void;
          finish: (commit: boolean) => void;
        }
      | undefined;
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
  const snapId = $props.id();
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
    edge: { cue: Cue; boundary: 'start' | 'end' } | null;
    anchor: number;
    left: number;
    width: number;
    start: number;
    span: number;
    duration: number;
    preview: [number, number] | null;
    guides: SnapGuide[];
  } | null>(null);
  let edge = $state.raw<{
    pointer: number;
    cue: Cue;
    boundary: 'start' | 'end';
    x: number;
    width: number;
    span: number;
    ready: boolean;
    latest: PointerEvent;
  } | null>(null);
  let hover = $state.raw<{ x: number; y: number } | null>(null);
  function locate(e: PointerEvent) {
    const rect = canvas.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }
  const cursor = $derived(
    waveformCursor(hover, { start, span, width }, cues, selected, range, {
      enabled,
      drag: edge ? 'edge' : gesture?.preview ? 'range' : null,
      canResize: !!onedge,
      canSelect: !!onselect,
    }),
  );
  function finishEdge(commit: boolean) {
    const previous = edge;
    edge = null;
    if (!previous) return;
    if (canvas.hasPointerCapture(previous.pointer))
      canvas.releasePointerCapture(previous.pointer);
    if (previous.ready) onedge?.finish(commit);
  }
  const displayedGaps = $derived(gaps ?? gapMarks(cues, timing.cueGapMs));
  const guides = $derived(
    gesture?.guides ?? (externalGuide ? [externalGuide] : []),
  );
  const showBuffers = $derived(
    !!gesture?.preview || editingCueId !== null || previewRange !== null,
  );
  const displayedRange = $derived(gesture?.preview ?? previewRange ?? range);
  // The parent receives fresh media-status objects on every poll. Compare the
  // position itself before invalidating this expensive canvas draw.
  const playhead = $derived(position);
  // Resolved once per theme or font change, not on every frame.
  const readPalette = () =>
    readCanvasPalette(getComputedStyle(document.documentElement));
  let palette = $state.raw<CanvasPalette>(readPalette());
  // Both cached rasters start with the opaque waveform background. Copy them at
  // backing-store resolution to preserve the original compositing and text.
  let cueLayer: HTMLCanvasElement;
  let markedLayer: HTMLCanvasElement;
  let gapStrip: HTMLCanvasElement;
  let gapGeometryDirty = true;
  let lastGaps: readonly GapMark[] | undefined;
  let lastCues: readonly Cue[] | undefined;
  // Quality reconciles its SvelteMap in place. Read visible keys in a tracked
  // derivation and keep the values that produced the last raster.
  const severities = $derived(cues.map((cue) => issues.get(String(cue.id))));
  let lastSeverities: typeof severities | undefined;
  let lastSelected: number | null = null;
  let checkCues = true;
  let cuesDirty = true;
  let marksDirty = true;
  let animation: number | null = null;
  const x = (t: number) => timeToX(t, start, span, width);
  const plotHeight = $derived(Math.max(20, height - 12));
  const scale = $derived(devicePixelRatio.current || 1);

  function resize(target: HTMLCanvasElement) {
    const w = Math.round(width * scale);
    const h = Math.round(height * scale);
    if (target.width !== w) target.width = w;
    if (target.height !== h) target.height = h;
  }

  function drawCues(context: CanvasRenderingContext2D) {
    const colors = palette;
    context.fillStyle = colors.background;
    context.fillRect(0, 0, width, height);
    for (const [index, cue] of cues.entries()) {
      const left = Math.max(0, x(cue.startMs / 1000));
      const right = Math.min(width, x(cue.endMs / 1000));
      if (right <= left) continue;
      context.fillStyle =
        cue.id === selected ? colors.cueFillSelected : colors.cueFill;
      context.fillRect(left, 20, right - left, plotHeight - 20);
      context.strokeStyle =
        cue.id === selected ? colors.cueLineSelected : colors.cueLine;
      context.lineWidth = cue.id === selected ? 2 : 1;
      context.strokeRect(
        left + 0.5,
        21,
        Math.max(0, right - left - 1),
        plotHeight - 22,
      );
      const severity = severities[index];
      if (severity) {
        context.fillStyle =
          severity === 'violation' ? colors.error : colors.warning;
        context.fillRect(
          left,
          21,
          Math.min(3, right - left),
          Math.max(0, plotHeight - 22),
        );
      }
      // An empty clip cannot contribute label pixels. Preserve even a partial
      // glyph in a narrow interval, and always issue the selected label.
      if (right - left <= 8 && cue.id !== selected) continue;
      context.save();
      context.beginPath();
      context.rect(
        left + 4,
        plotHeight - 22,
        Math.max(0, right - left - 8),
        20,
      );
      context.clip();
      context.fillStyle = colors.cueText;
      context.font = `11px ${colors.fontFamily}`;
      context.fillText(
        cue.text.replaceAll('\n', ' ↵ ') || 'Untitled cue',
        left + 5,
        plotHeight - 7,
      );
      context.restore();
    }
    context.font = `10px ${colors.fontFamily}`;
    context.fillStyle = colors.rulerText;
    context.strokeStyle = colors.rulerLine;
    const tick =
      Math.pow(10, Math.floor(Math.log10(span / 5))) *
      ([1, 2, 5, 10].find(
        (n) => n * Math.pow(10, Math.floor(Math.log10(span / 5))) >= span / 7,
      ) ?? 10);
    for (let t = Math.ceil(start / tick) * tick; t <= start + span; t += tick) {
      const px = x(t);
      context.beginPath();
      context.moveTo(px, 20);
      context.lineTo(px, plotHeight);
      context.stroke();
      context.fillText(`${t.toFixed(span < 5 ? 2 : 1)} s`, px + 3, 13);
    }
    const center = (plotHeight + 8) / 2;
    const amplitude = Math.max(10, (plotHeight - 58) / 2);
    context.strokeStyle = colors.wave;
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
  }

  function drawMarks(context: CanvasRenderingContext2D) {
    const colors = palette;
    const buffers = bufferVisibility(showBuffers);
    if (displayedRange) {
      context.fillStyle = colors.rangeFill;
      context.fillRect(
        x(displayedRange[0]),
        20,
        x(displayedRange[1]) - x(displayedRange[0]),
        plotHeight - 20,
      );
      context.strokeStyle = colors.rangeLine;
      for (const t of displayedRange) {
        context.beginPath();
        context.moveTo(x(t), 20);
        context.lineTo(x(t), plotHeight);
        context.stroke();
      }
    }
    if (buffers.shotBands || buffers.cueBands) {
      const band = (from: number, to: number, color: string) => {
        const left = Math.max(0, x(from));
        const right = Math.min(width, x(to));
        if (right <= left) return;
        hatch(context, left, right, 20, plotHeight, color);
      };
      if (buffers.cueBands)
        for (const cue of cues) {
          // The moving cue is not its own magnetic neighbor.
          if (cue.id === editingCueId) continue;
          band(
            (cue.startMs - timing.cueGapMs) / 1000,
            cue.startMs / 1000,
            colors.cueBuffer,
          );
          band(
            cue.endMs / 1000,
            (cue.endMs + timing.cueGapMs) / 1000,
            colors.cueBuffer,
          );
        }
      if (buffers.shotBands)
        for (const cut of sceneCuts)
          band(
            cut - timing.sceneBeforeMs / 1000,
            cut + timing.sceneAfterMs / 1000,
            colors.cutBuffer,
          );
    }
    drawTimelineMarks(context, colors, {
      width,
      height,
      start,
      span,
      cuts: sceneCuts,
      cues,
      timing: warningTiming ?? timing,
    });
    paintGapStrip(context);
  }

  // Reopening and saving can deliver a fresh snapshot with the same visible
  // cues. Compare the fields that actually reach the raster, in paint order.
  function cuePixelsChanged() {
    const old = lastCues;
    const oldSeverities = lastSeverities;
    const oldSelected = lastSelected;
    lastCues = cues;
    lastSeverities = severities;
    lastSelected = selected;
    if (!old || !oldSeverities || old.length !== cues.length) return true;
    if (
      old === cues &&
      oldSeverities === severities &&
      selected === oldSelected
    )
      return false;
    return cues.some((cue, i) => {
      const prior = old[i]!;
      return (
        prior.id !== cue.id ||
        prior.startMs !== cue.startMs ||
        prior.endMs !== cue.endMs ||
        prior.text !== cue.text ||
        (prior.id === oldSelected) !== (cue.id === selected) ||
        oldSeverities[i] !== severities[i]
      );
    });
  }

  // Gap labels depend only on neighboring times and the warning policy.
  // Keep the opaque bottom strip when a text-only edit replaces every cue.
  function paintGapStrip(context: CanvasRenderingContext2D) {
    const offset = plotHeight * scale;
    // A partially covered edge blends with the waveform underneath. Draw
    // directly there instead of composing independently antialiased layers.
    if (
      !Number.isInteger(offset) ||
      offset >= markedLayer.height ||
      width * scale < markedLayer.width
    ) {
      drawGaps(
        context,
        palette,
        displayedGaps,
        start,
        span,
        width,
        height,
        plotHeight,
      );
      return;
    }
    const old = lastGaps;
    if (
      gapGeometryDirty ||
      !old ||
      (old !== displayedGaps &&
        (old.length !== displayedGaps.length ||
          displayedGaps.some((gap, i) => {
            const prior = old[i]!;
            return (
              gap.fromMs !== prior.fromMs ||
              gap.toMs !== prior.toMs ||
              gap.milliseconds !== prior.milliseconds ||
              gap.severity !== prior.severity
            );
          })))
    ) {
      gapStrip ??= document.createElement('canvas');
      if (gapStrip.width !== markedLayer.width)
        gapStrip.width = markedLayer.width;
      if (gapStrip.height !== markedLayer.height - offset)
        gapStrip.height = markedLayer.height - offset;
      const strip = gapStrip.getContext('2d');
      if (!strip) {
        drawGaps(
          context,
          palette,
          displayedGaps,
          start,
          span,
          width,
          height,
          plotHeight,
        );
        return;
      }
      strip.setTransform(scale, 0, 0, scale, 0, -offset);
      strip.clearRect(
        0,
        plotHeight,
        gapStrip.width / scale,
        gapStrip.height / scale,
      );
      drawGaps(
        strip,
        palette,
        displayedGaps,
        start,
        span,
        width,
        height,
        plotHeight,
      );
      gapGeometryDirty = false;
    }
    lastGaps = displayedGaps;
    context.save();
    context.resetTransform();
    context.drawImage(gapStrip, 0, offset);
    context.restore();
  }

  function drawOverlay(context: CanvasRenderingContext2D) {
    const colors = palette;
    drawGuides(context, colors, guides, start, span, width, plotHeight);
    if (playhead === null) return;
    context.strokeStyle = colors.playhead;
    context.lineWidth = 2;
    context.beginPath();
    context.moveTo(x(playhead), 18);
    context.lineTo(x(playhead), height);
    context.stroke();
  }
  function draw() {
    animation = null;
    if (!canvas || width <= 0 || height <= 0) return;
    cueLayer ??= document.createElement('canvas');
    markedLayer ??= document.createElement('canvas');
    if (checkCues) {
      const changed = cuePixelsChanged();
      cuesDirty ||= changed;
      checkCues = false;
    }
    if (cuesDirty) {
      resize(cueLayer);
      const context = cueLayer.getContext('2d');
      if (!context) return;
      context.save();
      context.setTransform(scale, 0, 0, scale, 0, 0);
      context.clearRect(0, 0, cueLayer.width / scale, cueLayer.height / scale);
      drawCues(context);
      context.restore();
      cuesDirty = false;
      marksDirty = true;
    }
    if (marksDirty) {
      resize(markedLayer);
      const context = markedLayer.getContext('2d');
      if (!context) return;
      context.clearRect(0, 0, markedLayer.width, markedLayer.height);
      context.drawImage(cueLayer, 0, 0);
      context.save();
      context.setTransform(scale, 0, 0, scale, 0, 0);
      drawMarks(context);
      context.restore();
      marksDirty = false;
    }
    resize(canvas);
    const context = canvas.getContext('2d');
    if (!context) return;
    context.clearRect(0, 0, canvas.width, canvas.height);
    context.drawImage(markedLayer, 0, 0);
    context.save();
    context.setTransform(scale, 0, 0, scale, 0, 0);
    drawOverlay(context);
    context.restore();
  }

  function scheduleDraw() {
    animation ??= requestAnimationFrame(draw);
  }
  $effect(() => {
    void start;
    void span;
    void palette;
    void width;
    void height;
    void scale;
    gapGeometryDirty = true;
    cuesDirty = true;
    scheduleDraw();
  });
  $effect(() => {
    void view;
    cuesDirty = true;
    scheduleDraw();
  });
  $effect(() => {
    void cues;
    void selected;
    void severities;
    checkCues = true;
    scheduleDraw();
  });
  $effect(() => {
    void showBuffers;
    void editingCueId;
    void timing;
    void warningTiming;
    void sceneCuts;
    void displayedGaps;
    void displayedRange;
    marksDirty = true;
    scheduleDraw();
  });
  $effect(() => {
    void playhead;
    void guides;
    scheduleDraw();
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
    const unwatch = watchCanvasPalette(document.documentElement, () => {
      palette = readPalette();
    });
    return () => {
      if (animation !== null) cancelAnimationFrame(animation);
      unwatch();
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
    finishEdge(false);
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
    if (
      gesture ||
      edge ||
      !enabled ||
      !e.isPrimary ||
      e.button !== 0 ||
      duration <= 0
    )
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
      cueId:
        e.clientY - rect.top >= 20
          ? (cueAt(cues, selected, time(e, mapping))?.id ?? null)
          : null,
      edge: onedge ? edgeAt(locate(e), mapping, cues) : null,
      preview: null,
      guides: [],
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
    mapping.guides = [
      snapGuide(first, cueNumbers),
      snapGuide(last, cueNumbers),
    ].filter((guide): guide is SnapGuide => guide !== null);
    // Preserve the original fractional pointer mapping when no target is selected.
    return [
      first.target ? first.milliseconds / 1000 : raw[0],
      last.target ? last.milliseconds / 1000 : raw[1],
    ];
  }
  function move(e: PointerEvent) {
    hover = locate(e);
    if (edge?.pointer === e.pointerId) {
      edge.latest = e;
      if (edge.ready) {
        const delta = e.clientX - edge.x;
        const original =
          edge.cue[edge.boundary === 'start' ? 'startMs' : 'endMs'];
        onedge?.preview(
          edge.boundary,
          Math.round(
            clamp(
              original + (delta * edge.span * 1000) / edge.width,
              0,
              duration * 1000,
            ),
          ),
          {
            bypass: e.altKey || delta === 0,
            millisecondsPerPixel: (edge.span * 1000) / edge.width,
          },
        );
      }
      return;
    }
    if (!gesture || gesture.pointer !== e.pointerId) return;
    if (gesture.edge && onedge && Math.abs(e.clientX - gesture.anchorX) > 3) {
      const captured = {
        pointer: gesture.pointer,
        cue: gesture.edge.cue,
        boundary: gesture.edge.boundary,
        x: gesture.anchorX,
        width: gesture.width,
        span: gesture.span,
        ready: false,
        latest: e,
      };
      // Keep capture while transferring the pending click to the timing owner.
      // Below the threshold, release retains ordinary cue selection and seeking.
      gesture = null;
      ongesture?.(false);
      edge = captured;
      void onedge
        .begin(captured.cue.id, () => edge === captured)
        .then((ready) => {
          if (edge === captured) {
            captured.ready = ready;
            if (ready) move(captured.latest);
            else finishEdge(false);
          } else if (ready) onedge?.finish(false);
        });
      return;
    }
    const preview =
      Math.abs(e.clientX - gesture.anchorX) > 3
        ? snappedRange(e, gesture)
        : null;
    gesture = { ...gesture, preview, guides: preview ? gesture.guides : [] };
  }
  function end(e: PointerEvent) {
    if (edge?.pointer === e.pointerId) {
      move(e);
      finishEdge(true);
      return;
    }
    const previous = gesture;
    if (!previous || previous.pointer !== e.pointerId) return;
    // Clear capture ownership before publishing so capture loss cannot cancel
    // another gesture. Until release, the selected playback range is unchanged.
    cancel();
    if (Math.abs(e.clientX - previous.anchorX) <= 3) {
      const clickedCue = cues.find((cue) => cue.id === previous.cueId);
      const alreadyRanged = exactRange(clickedCue, selected, range);
      if (previous.cueId !== null && onselect && !alreadyRanged)
        onselect(previous.cueId);
      else {
        const seconds = time(e, previous);
        if (range && onclearrange && !cueAt(previous.cues, selected, seconds))
          onclearrange(seconds);
        else onseek(seconds);
      }
    } else onrange(snappedRange(e, previous));
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
      e.clientY - rect.top >= 20
        ? (cueAt(cues, selected, t)?.id ?? null)
        : null,
      !!range && t >= range[0] && t <= range[1],
    );
  }
  function cancelPointer(e: PointerEvent) {
    if (edge?.pointer === e.pointerId) finishEdge(false);
    if (gesture?.pointer === e.pointerId) cancel();
  }
</script>

<svelte:window
  onblur={cancel}
  onkeydown={(e) => {
    if (e.key === 'Escape' && (gesture || edge)) {
      e.preventDefault();
      cancel();
    }
  }}
/>
<canvas
  bind:this={canvas}
  onkeydown={(e) => {
    if (
      e.key === 'Escape' &&
      enabled &&
      range &&
      !gesture &&
      !edge &&
      !previewRange &&
      !e.isComposing &&
      !e.defaultPrevented &&
      onclearrange
    ) {
      e.preventDefault();
      onclearrange();
    }
  }}
  aria-label="Audio waveform: click a cue to select it, click empty space to clear the range and seek, drag to select a range"
  aria-describedby={guides.length ? snapId : undefined}
  title="Click a cue to select its range; click inside it again to seek. Scroll horizontally to pan; vertically to zoom. Shift+wheel also pans."
  tabindex="0"
  oncontextmenu={contextMenu}
  onpointerdown={begin}
  style:cursor
  onpointerenter={(e) => (hover = locate(e))}
  onpointerleave={() => (hover = null)}
  onpointermove={move}
  onpointerup={end}
  onpointercancel={cancelPointer}
  onlostpointercapture={cancelPointer}
></canvas>

<span id={snapId} class="snap-description" role="status"
  >{guides.map((guide) => `Snapped to ${guide.label}`).join('; ')}</span
>

<style>
  .snap-description {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
    border: 0;
  }
  canvas {
    display: block;
    width: 100%;
    height: 100%;
    min-height: 0;
    touch-action: none;
  }
</style>
