import type { QualityMetric, QualityRule, GapView } from './client';
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
  { id: 'scene_start', label: 'Start near scene cut' },
  { id: 'scene_end', label: 'End near scene cut' },
  { id: 'spelling', label: 'Spelling' },
  { id: 'typography', label: 'Typography' },
  { id: 'capitalization', label: 'Capitalization' },
  { id: 'dialogue_marker', label: 'Dialogue marker' },
  { id: 'language_coverage', label: 'Language coverage' },
];
export function sceneCoverage(
  coverage: GapView['coverage'] | undefined,
): string {
  switch (coverage) {
    case 'checked':
      return 'Scene buffers checked against the completed Standard scan. Visual detection is heuristic and can miss cuts.';
    case 'reduced':
      return 'Scene buffers checked against the completed Reduced scan. Reduced detection can miss cuts; review proposed timings against the video.';
    case 'partial':
      return 'Scene coverage is partial or still scanning. Scene checks and repairs require a completed scan.';
    case 'unavailable':
      return 'Current completed scene data is unavailable. Run a scene scan to check or repair scene timing.';
    case 'noMedia':
      return 'No media is attached. Scene timing has not been checked; scene repair is unavailable.';
    case 'notRequired':
      return 'Scene checks are disabled because both scene buffers are zero.';
    default:
      return 'Scene coverage has not yet been checked.';
  }
}
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
