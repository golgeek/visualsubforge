// Generated from Rust Serde output; do not edit.
import type {
  TimingPreset,
  DocumentAction,
  DocumentSnapshot,
  RecoveryStatus,
} from '../../src/lib/client';
export const preset = {
  version: 1,
  name: 'Contract sample',
  timing: {
    enabled: false,
    snapCues: true,
    snapScenes: true,
    cueGapMs: 80,
    sceneBeforeMs: 130,
    sceneAfterMs: 130,
    snapDistancePx: 8,
  },
} satisfies TimingPreset;
export const documentActions = [
  {
    kind: 'status',
  },
  {
    afterId: null,
    endMs: 2000,
    kind: 'create',
    startMs: 1000,
    text: 'A\nB',
  },
  {
    endMs: 3000,
    kind: 'create',
    startMs: 2100,
    text: 'Missing optional ID',
  },
  {
    cueId: 1,
    endMs: 2200,
    kind: 'edit',
    startMs: 1100,
    text: 'Edited',
  },
  {
    cueId: 1,
    kind: 'delete',
  },
  {
    atMs: 1500,
    cueId: 1,
    kind: 'split',
  },
  {
    firstId: 1,
    kind: 'merge',
    secondId: 2,
  },
  {
    kind: 'save',
    path: null,
  },
  {
    kind: 'save',
    path: 'copy.srt',
  },
  {
    discard: false,
    kind: 'open',
    path: 'source.srt',
  },
  {
    discard: true,
    id: 'session-example',
    kind: 'recover',
  },
  {
    discard: false,
    kind: 'close',
  },
  {
    kind: 'undo',
  },
  {
    kind: 'redo',
  },
] satisfies DocumentAction[];
export const emptyDocument = {
  canRedo: false,
  canUndo: false,
  cues: [],
  dirty: false,
  generation: 0,
  loaded: false,
  path: null,
  requiresSaveAs: false,
  revision: 0,
} satisfies DocumentSnapshot;
export const createdDocument = {
  canRedo: false,
  canUndo: true,
  cues: [
    {
      endMs: 2000,
      id: 1,
      startMs: 1000,
      text: 'A\nB',
    },
  ],
  dirty: true,
  generation: 1,
  loaded: true,
  path: null,
  requiresSaveAs: true,
  revision: 1,
} satisfies DocumentSnapshot;
export const recovery = {
  entries: [
    {
      capturedMs: 9007199254740991,
      error: null,
      id: 'session-example',
      revision: 42,
      sourcePath: null,
    },
    {
      capturedMs: null,
      error: 'Corrupt record',
      id: 'session-corrupt',
      revision: null,
      sourcePath: null,
    },
  ],
  error: null,
} satisfies RecoveryStatus;
