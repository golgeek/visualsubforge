<script lang="ts">
  let {
    value,
    maximum,
    label,
  }: { value: number | null; maximum: number; label: string } = $props();
  const fraction = $derived(
    value === null || maximum <= 0
      ? 0
      : Math.min(1, Math.max(0, value / maximum)),
  );
</script>

<div
  class="meter"
  role="meter"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={maximum}
  aria-valuenow={value ?? 0}
  aria-valuetext={value === null ? 'Unavailable' : String(value)}
>
  <span style:width={`${fraction * 100}%`}></span>
</div>

<style>
  .meter {
    height: 3px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  span {
    display: block;
    height: 100%;
    background: var(--cue);
  }
</style>
