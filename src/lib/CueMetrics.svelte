<script lang="ts">
  import type { QualityMetric } from './client';
  import { rate, rateReason, rating } from './quality-display';
  let { value, label }: { value: QualityMetric; label: string } = $props();
</script>

<div class="cue-metrics" aria-label={`${label} cue metrics`}>
  <strong>{label}</strong>
  <span
    >{value.data.durationMs === null
      ? 'Duration unavailable'
      : `${(value.data.durationMs / 1000).toFixed(3)} s`}</span
  >
  <span title={rateReason(value.data.cps)}>CPS {rate(value.data.cps)}</span>
  <span title={rateReason(value.data.rs)}
    >RS {rate(value.data.rs)} · {rating(value.data.rating)}</span
  >
  <span>{value.data.characters} UTF-16 units</span>
  <span>{value.lineCount} lines · longest {value.longestLine}</span>
  <span
    title={value.linesTruncated
      ? 'First 20 line lengths; full count and longest line shown above'
      : 'Source line lengths'}
    >Lines: {value.data.lineCharacters.join(' / ')}{value.linesTruncated
      ? ' / …'
      : ''}</span
  >
  {#if value.data.gapMs !== null}<span>Next gap {value.data.gapMs} ms</span
    >{/if}
  {#if value.data.effectiveDurationMs !== null && value.data.effectiveDurationMs !== value.data.durationMs}<span
      >Reading time {(value.data.effectiveDurationMs / 1000).toFixed(3)} s</span
    >{/if}
</div>

<style>
  .cue-metrics {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    font-size: 11px;
    color: #bfd3df;
  }
  strong {
    color: #92e0d2;
  }
</style>
