<script lang="ts">
  import { onMount } from 'svelte';
  import type { Cue } from './client';
  import type { CueBoundary } from './cue-timing';
  import { clamp, formatTime } from './timeline';
  let {
    cue,
    start,
    span,
    duration,
    enabled,
    onbegin,
    onpreview,
    onfinish,
  }: {
    cue: Cue;
    start: number;
    span: number;
    duration: number;
    enabled: boolean;
    onbegin: () => boolean;
    onpreview: (
      boundary: CueBoundary,
      milliseconds: number,
      pointer?: { bypass: boolean; millisecondsPerPixel: number },
    ) => void;
    onfinish: (commit: boolean) => void;
  } = $props();
  const boundaries: CueBoundary[] = ['start', 'end'];
  let layer: HTMLDivElement;
  let gesture: {
    boundary: CueBoundary;
    pointer: number | null;
    element: HTMLElement;
    width: number;
    span: number;
    originX: number;
    originalMs: number;
    dragging: boolean;
  } | null = null;
  function finish(commit: boolean) {
    const previous = gesture;
    if (!previous) return;
    gesture = null;
    if (
      previous.pointer !== null &&
      previous.element.hasPointerCapture(previous.pointer)
    )
      previous.element.releasePointerCapture(previous.pointer);
    onfinish(commit);
  }
  function begin(
    boundary: CueBoundary,
    element: HTMLElement,
    pointer: number | null,
    originX = 0,
  ): boolean {
    if (gesture || !enabled || !onbegin()) return false;
    const rect = layer.getBoundingClientRect();
    gesture = {
      boundary,
      pointer,
      element,
      width: rect.width,
      span,
      originX,
      originalMs: boundary === 'start' ? cue.startMs : cue.endMs,
      dragging: false,
    };
    element.focus({ preventScroll: true });
    return true;
  }
  function down(event: PointerEvent, boundary: CueBoundary) {
    if (!event.isPrimary || event.button !== 0) return;
    event.preventDefault();
    const element = event.currentTarget as HTMLElement;
    if (begin(boundary, element, event.pointerId, event.clientX))
      element.setPointerCapture(event.pointerId);
  }
  function move(event: PointerEvent) {
    if (!gesture || gesture.pointer !== event.pointerId) return;
    const delta = event.clientX - gesture.originX;
    // Grabbing anywhere on the handle preserves its time. Ignore small click
    // jitter until a drag starts; thereafter keep the full displacement so
    // returning to the origin restores the exact original boundary.
    if (!gesture.dragging && Math.abs(delta) < 3) return;
    gesture.dragging = true;
    const milliseconds =
      delta === 0
        ? gesture.originalMs
        : Math.round(
            clamp(
              gesture.originalMs +
                (delta / gesture.width) * gesture.span * 1000,
              0,
              duration * 1000,
            ),
          );
    onpreview(gesture.boundary, milliseconds, {
      bypass: event.altKey || delta === 0,
      millisecondsPerPixel: (gesture.span * 1000) / gesture.width,
    });
  }
  function cancelPointer(event: PointerEvent) {
    // Another pointer or a capture-loss event from the other handle does not
    // own this gesture. Pointer events cannot cancel a held-key edit either.
    if (
      gesture?.pointer === event.pointerId &&
      gesture.element === event.currentTarget
    )
      finish(false);
  }
  function key(event: KeyboardEvent, boundary: CueBoundary) {
    if (event.key === 'Escape') {
      event.preventDefault();
      finish(false);
      return;
    }
    if (
      !['ArrowLeft', 'ArrowRight'].includes(event.key) ||
      event.ctrlKey ||
      event.metaKey ||
      event.isComposing
    )
      return;
    event.preventDefault();
    // Cancellation ends the whole held-key gesture. OS repeats after Escape,
    // blur or resize must not silently start another edit before key release.
    if (!gesture && event.repeat) return;
    if (!gesture && !begin(boundary, event.currentTarget as HTMLElement, null))
      return;
    if (gesture?.pointer !== null || gesture.boundary !== boundary) return;
    const step = event.altKey ? 1 : event.shiftKey ? 100 : 10;
    onpreview(
      boundary,
      (boundary === 'start' ? cue.startMs : cue.endMs) +
        (event.key === 'ArrowLeft' ? -step : step),
    );
  }
  onMount(() => {
    const observer = new ResizeObserver(() => {
      if (gesture) finish(false);
    });
    observer.observe(layer);
    return () => {
      observer.disconnect();
      finish(false);
    };
  });
</script>

<svelte:window
  onblur={() => finish(false)}
  onkeydown={(e) => {
    if (e.key === 'Escape' && gesture) {
      e.preventDefault();
      finish(false);
    }
  }}
/>
<div class="timing-layer" bind:this={layer}>
  {#each boundaries as boundary (boundary)}
    {@const time = boundary === 'start' ? cue.startMs : cue.endMs}
    {#if time / 1000 >= start && time / 1000 <= start + span}
      <button
        class="boundary"
        class:end={boundary === 'end'}
        style:left={`clamp(2px, ${clamp(((time / 1000 - start) / span) * 100, 0, 100)}%, calc(100% - 2px))`}
        aria-label={`Cue ${boundary} boundary ${formatTime(time)}`}
        aria-disabled={!enabled}
        title={`${boundary === 'start' ? 'Start' : 'End'} ${formatTime(time)} · drag or Left/Right: 10 ms, Shift: 100 ms, Alt: 1 ms; Alt-drag bypasses snapping; Escape cancels`}
        onpointerdown={(e) => down(e, boundary)}
        onpointermove={move}
        onpointerup={(e) => {
          if (gesture?.pointer === e.pointerId) {
            move(e);
            finish(true);
          }
        }}
        onpointercancel={cancelPointer}
        onlostpointercapture={cancelPointer}
        onkeydown={(e) => key(e, boundary)}
        onkeyup={(e) => {
          if (
            gesture?.pointer === null &&
            ['ArrowLeft', 'ArrowRight'].includes(e.key)
          )
            finish(true);
        }}
        onblur={(e) => {
          // Focusing the other handle blurs this one after its gesture began.
          if (gesture?.element === e.currentTarget) finish(false);
        }}><span class="grip"></span></button
      >
    {/if}
  {/each}
</div>

<style>
  .timing-layer {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .boundary {
    position: absolute;
    top: 22px;
    bottom: 0;
    width: 10px;
    padding: 0;
    transform: translateX(-50%);
    border: 0;
    border-left: 2px solid #a1c9ff;
    background: #77acff20;
    color: white;
    cursor: ew-resize;
    pointer-events: auto;
    touch-action: none;
    user-select: none;
    border-radius: 3px;
  }
  .grip {
    display: block;
    width: 5px;
    height: 18px;
    background: #a1c9ff;
    border-radius: 2px;
    position: absolute;
    top: 50%;
    left: -3px;
  }
  .boundary[aria-disabled='true'] {
    opacity: 0.6;
  }
  .boundary:focus-visible {
    outline: 2px solid #ffd873;
    outline-offset: 1px;
  }
</style>
