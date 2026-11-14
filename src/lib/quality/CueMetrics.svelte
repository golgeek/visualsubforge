<script lang="ts">
  import type { QualityMetric, QualityProfile } from '../client';
  import Meter from '../ui/Meter.svelte';
  import { rate, rateReason, rating } from './quality-display';
  let {
    value,
    label,
    profile,
  }: { value: QualityMetric; label: string; profile: QualityProfile } =
    $props();
  const rs = $derived(
    value.data.rs.status === 'finite' ? value.data.rs.value : null,
  );
  const cps = $derived(
    value.data.cps.status === 'finite' ? value.data.cps.value : null,
  );
</script>

<div class="cue-metrics" aria-label={`${label} cue metrics`}>
  <strong>{label}</strong>
  <div
    class="reading"
    title={rateReason(value.data.rs) || rating(value.data.rating)}
  >
    <span>RS {rate(value.data.rs)}</span>
    <Meter
      value={rs}
      maximum={profile.rules.speed.maximumRsExclusive ?? Math.max(1, rs ?? 1)}
      label={`${label} reading speed`}
    />
  </div>
  <div class="reading" title={rateReason(value.data.cps)}>
    <span>CPS {rate(value.data.cps)}</span>
    <Meter
      value={cps}
      maximum={profile.rules.speed.maximumCps ?? Math.max(1, cps ?? 1)}
      label={`${label} characters per second`}
    />
  </div>
  <div class="reading">
    <span
      >{value.data.durationMs === null
        ? 'Duration unavailable'
        : `${(value.data.durationMs / 1000).toFixed(3)} s`}</span
    >
    <Meter
      value={value.data.durationMs}
      maximum={profile.rules.duration.maximumMs}
      label={`${label} duration in milliseconds`}
    />
  </div>
  <span
    >{value.data.characters} characters · {value.lineCount} lines · longest {value.longestLine}</span
  >
</div>

<style>
  .cue-metrics {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 28px;
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
  }
  strong {
    color: var(--text);
  }
  .reading {
    width: 90px;
    flex-shrink: 0;
  }
  .reading span {
    display: block;
    margin-bottom: 3px;
  }
</style>
