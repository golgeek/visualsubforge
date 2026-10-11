<script lang="ts">
  let {
    label,
    vertical = false,
    value,
    onchange,
  }: {
    label: string;
    vertical?: boolean;
    value: number;
    onchange: (value: number) => void;
  } = $props();
  let start = 0;
  let original = 0;
  let size = 1;
  function begin(e: PointerEvent) {
    if (!e.isPrimary || e.button !== 0) return;
    // Resizing must not start WebKit's text-selection/HTML drag operation.
    e.preventDefault();
    const element = e.currentTarget as HTMLButtonElement;
    element.focus({ preventScroll: true });
    element.setPointerCapture(e.pointerId);
    start = vertical ? e.clientX : e.clientY;
    original = value;
    const rect = element.parentElement!.getBoundingClientRect();
    size = vertical ? rect.width : rect.height;
  }
  function move(e: PointerEvent) {
    if (!(e.currentTarget as HTMLElement).hasPointerCapture(e.pointerId))
      return;
    onchange(
      original + (((vertical ? e.clientX : e.clientY) - start) / size) * 100,
    );
  }
</script>

<button
  class:vertical
  class="splitter"
  aria-label={label}
  title={`${label} (drag or use arrow keys)`}
  onpointerdown={begin}
  onpointermove={move}
  onkeydown={(e) => {
    if (['ArrowLeft', 'ArrowUp', 'ArrowRight', 'ArrowDown'].includes(e.key)) {
      e.preventDefault();
      onchange(value + (['ArrowLeft', 'ArrowUp'].includes(e.key) ? -2 : 2));
    }
  }}
></button>

<style>
  .splitter {
    border: 0;
    padding: 0;
    background: #31434e;
    cursor: row-resize;
    touch-action: none;
    user-select: none;
    min-width: 6px;
    min-height: 6px;
    border-radius: 0;
    z-index: 3;
  }
  .vertical {
    cursor: col-resize;
  }
  .splitter:hover,
  .splitter:focus-visible {
    background: #74c9b7;
    outline: none;
  }
</style>
