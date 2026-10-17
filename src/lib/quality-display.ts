import type { QualityMetric, QualityRule } from './client';
export const qualityRules: { id: QualityRule; label: string }[] = [
  { id: 'empty', label: 'Empty cue' },
  { id: 'duration_short', label: 'Too short' },
  { id: 'duration_long', label: 'Too long' },
  { id: 'cps_low', label: 'CPS too low' },
  { id: 'cps_high', label: 'CPS too high' },
  { id: 'rs_low', label: 'RS too low' },
  { id: 'rs_high', label: 'RS too high' },
  { id: 'speed_unavailable', label: 'Speed unavailable' },
  { id: 'line_count', label: 'Too many lines' },
  { id: 'line_length', label: 'Line length' },
  { id: 'overlap', label: 'Overlap' },
  { id: 'gap', label: 'Minimum gap' },
];
export function rate(value: QualityMetric['data']['cps']): string {
  return value.status === 'finite' && value.value !== null
    ? value.value.toFixed(1)
    : '—';
}
export function rateReason(value: QualityMetric['data']['cps']): string {
  if (value.status === 'finite') return '';
  return {
    empty_text: 'empty text',
    invalid_duration: 'invalid duration',
    reading_allowance: 'duration does not exceed the 500 ms reading allowance',
  }[value.reason];
}
export function rating(value: QualityMetric['data']['rating']): string {
  return value?.replaceAll('_', ' ') ?? 'unavailable';
}
