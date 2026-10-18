// Generated from Rust profiles; do not edit.
import type { QualityProfile } from '../client';
export const builtinQualityProfiles = [
  {
    version: 2,
    kind: 'quality',
    id: 'vsf-default',
    name: 'VSF authoring defaults',
    revision: 1,
    publisher: 'VisualSubForge',
    source:
      'VSF quality contract v1; authoring aid, not a broadcaster standard',
    language: 'und',
    textAlgorithm: 'vss_utf16_v1',
    speedBasis: 'ordinary_v1',
    timing: {
      cueGapMs: 80,
      sceneBeforeMs: 130,
      sceneAfterMs: 130,
    },
    rules: {
      duration: {
        enabled: true,
        minimumMs: 800,
        maximumMs: 7000,
      },
      speed: {
        enabled: true,
        minimumRs: 5,
        maximumRsExclusive: 35,
        minimumCps: null,
        maximumCps: 20,
      },
      lines: {
        enabled: true,
        maximumLines: 2,
        singleLinePreferred: 40,
        maximumCharacters: 42,
      },
      gaps: true,
      empty: true,
    },
    limitations: [],
  },
  {
    version: 2,
    kind: 'quality',
    id: 'sw3-uk-vo-subset',
    name: 'SW3 UK VO — archived subset',
    revision: 1,
    publisher: 'VSF; derived from archived vss-plugins policy',
    source:
      'vss-plugins Hg 48928de4cfa6f28c4b90b51a8c65f30803aab650, presets/SW3_UK_VO.ini and jsplugin/common/common.js; VSS host 71778b9115e57c03e9be85234318c2004ac427b3',
    language: 'en',
    textAlgorithm: 'vss_utf16_v1',
    speedBasis: 'vss_overlap_v1',
    timing: {
      cueGapMs: 160,
      sceneBeforeMs: 120,
      sceneAfterMs: 120,
    },
    rules: {
      duration: {
        enabled: true,
        minimumMs: 800,
        maximumMs: 5000,
      },
      speed: {
        enabled: true,
        minimumRs: 5,
        maximumRsExclusive: 35,
        minimumCps: null,
        maximumCps: null,
      },
      lines: {
        enabled: true,
        maximumLines: 2,
        singleLinePreferred: 37,
        maximumCharacters: 40,
      },
      gaps: true,
      empty: true,
    },
    limitations: [
      'Strict duration only: interactive minimum, neighbor/scene exemptions and historical tolerance are not implemented.',
      'Scene/filter/sticky-blank and consecutive-fast/scene-sequence checks are not implemented.',
      'Typography, capitalization, dialogue, tag validity, linguistic splitting and pixel-width checks are not implemented.',
      'Independent source-derived metrics; no exhaustive executed VSS compatibility claim or full SW compliance.',
    ],
  },
] satisfies QualityProfile[];
