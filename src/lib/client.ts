import { commands } from './generated/ipc';
import type {
  ImportedTimingPreset,
  TimingPreset,
  TimingSettings,
} from './generated/ipc';
export type { TimingPreset, TimingSettings } from './generated/ipc';
import { defaultTiming } from './magnetic-timing';
import { invoke, isTauri } from '@tauri-apps/api/core';

// Read once; diagnostics never delay a document request or alter its result.
// The native process explicitly opts in with VSF_WORKFLOW_TRACE=1.
let traceWorkflow = false;
if (isTauri()) {
  void invoke<boolean>('workflow_trace_enabled')
    .then((enabled) => (traceWorkflow = enabled))
    .catch(() => {});
}

// Observe only explicit Apply inputs; never retain the DOM event or cue text.
// Event creation, handler delivery and wall-clock correlation remain distinct.
export function traceApplyInput(
  event: Event,
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
): void {
  if (!traceWorkflow || (event.type !== 'click' && event.type !== 'keydown'))
    return;
  const monoBeforeMs = performance.now();
  const wallUnixMs = Date.now();
  const monoAfterMs = performance.now();
  const sample = {
    generation: document.generation,
    beforeRevision: document.revision,
    eventType: event.type,
    trusted: event.isTrusted,
    eventMonoMs: event.timeStamp,
    timeOriginMs: performance.timeOrigin,
    monoBeforeMs,
    wallUnixMs,
    monoAfterMs,
  };
  // Submit after the synchronous action handler has dispatched its real work.
  // Diagnostic failure must never change whether that edit succeeds.
  queueMicrotask(() => {
    void invoke('workflow_trace_input', { sample }).catch(() => {});
  });
}

export type MediaAction =
  | { kind: 'occlude'; occluded: boolean }
  | { kind: 'decode'; hardware: boolean }
  | {
      kind: 'layout';
      x: number;
      y: number;
      width: number;
      height: number;
      visible: boolean;
    }
  | { kind: 'open'; path: string; hardware: boolean }
  | { kind: 'close' | 'status' | 'clearLoop' }
  | { kind: 'pause'; paused: boolean }
  | { kind: 'seek'; seconds: number }
  | { kind: 'step'; backwards: boolean }
  | { kind: 'track'; id: number }
  | { kind: 'loop'; start: number; end: number };
export interface MediaStatus {
  session: number;
  available: boolean;
  loaded: boolean;
  paused: boolean;
  seeking: boolean;
  queuedAudioClock: boolean;
  positionSeconds: number | null;
  durationSeconds: number | null;
  audioTrack: number | null;
  audioTracks: { id: number; title: string; audioOrdinal: number }[];
  avSyncSeconds: number | null;
  renderedFrames: number;
  swapCounterAvailable: boolean;
  videoOutputConfigured: boolean;
  surfaceAboveWebview: boolean;
  hardwareDecoder: string | null;
  decoderDrops: number | null;
  outputDrops: number | null;
  error: string | null;
  subtitlePreviewError: string | null;
  previewDocumentGeneration: number | null;
  previewDocumentRevision: number | null;
}
export const emptyStatus: MediaStatus = {
  session: 0,
  available: false,
  loaded: false,
  paused: true,
  seeking: false,
  queuedAudioClock: false,
  positionSeconds: null,
  durationSeconds: null,
  audioTrack: null,
  audioTracks: [],
  avSyncSeconds: null,
  renderedFrames: 0,
  swapCounterAvailable: false,
  videoOutputConfigured: false,
  surfaceAboveWebview: false,
  hardwareDecoder: null,
  decoderDrops: null,
  outputDrops: null,
  error: null,
  subtitlePreviewError: null,
  previewDocumentGeneration: null,
  previewDocumentRevision: null,
};
// Handwritten contract mirrors media::Action/Status; only this module invokes Tauri.
export async function mediaAction(
  action: MediaAction,
  session: number,
): Promise<MediaStatus> {
  if (!isTauri()) return emptyStatus;
  return invoke<MediaStatus>('media_action', { action, session });
}

export interface Cue {
  id: number;
  startMs: number;
  endMs: number;
  text: string;
}
export interface DocumentSnapshot {
  generation: number;
  revision: number;
  loaded: boolean;
  path: string | null;
  dirty: boolean;
  requiresSaveAs: boolean;
  canUndo: boolean;
  canRedo: boolean;
  cues: Cue[];
}
export const emptyDocument: DocumentSnapshot = {
  generation: 0,
  revision: 0,
  loaded: false,
  path: null,
  dirty: false,
  requiresSaveAs: false,
  canUndo: false,
  canRedo: false,
  cues: [],
};
export type DocumentAction =
  | { kind: 'status' | 'undo' | 'redo' }
  | { kind: 'open'; path: string; discard: boolean }
  | { kind: 'close'; discard: boolean }
  | { kind: 'recover'; id: string; discard: boolean }
  | { kind: 'save'; path: string | null }
  | {
      kind: 'create';
      afterId: number | null;
      startMs: number;
      endMs: number;
      text: string;
    }
  | { kind: 'delete'; cueId: number }
  | { kind: 'split'; cueId: number; atMs: number }
  | { kind: 'merge'; firstId: number; secondId: number }
  | {
      kind: 'edit';
      cueId: number;
      startMs: number;
      endMs: number;
      text: string;
    };
export async function documentAction(
  action: DocumentAction,
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
): Promise<DocumentSnapshot> {
  const started = traceWorkflow ? performance.now() : null;
  const startedUnixMs = traceWorkflow ? Date.now() : null;
  const result = await invoke<DocumentSnapshot>('document_action', {
    action,
    generation: document.generation,
    revision: document.revision,
  });
  if (started !== null && action.kind !== 'status') {
    // Measure IPC through response decoding, before optional diagnostic IO.
    // Rendering the accepted UI and native subtitle presentation are separate.
    void invoke('workflow_trace_acknowledgment', {
      sample: {
        operation: action.kind,
        generation: result.generation,
        revision: result.revision,
        elapsedMs: performance.now() - started,
        startedUnixMs,
      },
    }).catch(() => {});
  }
  return result;
}
export type WorkspaceAction =
  | 'settings'
  | 'timingSettings'
  | 'showScenes'
  | 'scanScenes'
  | 'cancelSceneScan'
  | 'previousScene'
  | 'nextScene'
  | 'openProject'
  | 'saveProject'
  | 'saveProjectAs'
  | 'createCue'
  | 'deleteCue'
  | 'splitCue'
  | 'mergePrevious'
  | 'mergeNext'
  | 'apply'
  | 'openMedia'
  | 'openSubtitles'
  | 'save'
  | 'saveAs'
  | 'closeMedia'
  | 'closeSubtitles'
  | 'undo'
  | 'redo'
  | 'play'
  | 'previousFrame'
  | 'nextFrame'
  | 'playRange'
  | 'clearLoop'
  | 'resetLayout'
  | 'diagnostics'
  | 'quit';
export async function workspaceActions(
  callback: (action: WorkspaceAction) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  const { listen } = await import('@tauri-apps/api/event');
  return listen<WorkspaceAction>('workspace-action', (event) =>
    callback(event.payload),
  );
}
export function chooseFile(
  kind:
    | 'media'
    | 'subtitles'
    | 'saveSubtitles'
    | 'project'
    | 'saveProject'
    | 'timingPreset'
    | 'saveTimingPreset',
  current: string | null = null,
): Promise<string | null> {
  return invoke('choose_file', { kind, current });
}
export async function updateMenu(state: {
  diagnostics: boolean;
  showScenes: boolean;
  scenesRunning: boolean;
  scenesPending: boolean;
  previousScene: boolean;
  nextScene: boolean;
  available: boolean;
  media: boolean;
  document: boolean;
  undo: boolean;
  redo: boolean;
  range: boolean;
  busy: boolean;
  createCue: boolean;
  deleteCue: boolean;
  splitCue: boolean;
  mergePrevious: boolean;
  mergeNext: boolean;
}): Promise<void> {
  if (isTauri()) await invoke('update_menu', { state });
}
export function finishExit(
  document: DocumentSnapshot,
  discard: boolean,
): Promise<void> {
  return invoke('finish_exit', {
    generation: document.generation,
    revision: document.revision,
    discard,
  });
}
export interface WaveformView {
  generation: number;
  running: boolean;
  error: string | null;
  durationSeconds: number;
  containerOriginSeconds: number;
  audioStreamIndex: number | null;
  startSeconds: number;
  secondsPerPeak: number;
  peaks: [number, number][];
}
export function waveformStart(
  path: string,
  audioOrdinal: number,
  session: number,
): Promise<number> {
  return invoke('waveform_start', { path, audioOrdinal, session });
}
export function waveformView(
  generation: number,
  start: number,
  end: number,
  pixels: number,
): Promise<WaveformView> {
  return invoke('waveform_view', {
    generation,
    start,
    end,
    pixels: Math.min(4096, Math.max(1, Math.ceil(pixels))),
  });
}

export interface RecoveryStatus {
  entries: {
    id: string;
    sourcePath: string | null;
    capturedMs: number | null;
    revision: number | null;
    error: string | null;
  }[];
  error: string | null;
}
export function recoveryStatus(): Promise<RecoveryStatus> {
  return invoke('recovery_status');
}
export function discardRecovery(id: string): Promise<void> {
  return invoke('recovery_discard', { id });
}

export type SceneMode = 'keyframes' | 'visual';
export type SceneDetail = 'standard' | 'reduced';
export interface SceneView {
  mode: SceneMode | null;
  scanning: SceneMode | null;
  phase: 'candidates' | 'verifying' | 'full' | null;
  progress: number;
  partial: boolean;
  note: string | null;
  truncated: boolean;
  generation: number;
  running: boolean;
  error: string | null;
  total: number;
  cuts: number[];
  previous: number | null;
  next: number | null;
}
export function scenesStart(
  path: string,
  session: number,
  mode: SceneMode,
  detail: SceneDetail = 'standard',
): Promise<number> {
  return invoke('scenes_start', { path, session, mode, detail });
}
export function scenesCancel(session: number): Promise<number> {
  return invoke('scenes_cancel', { session });
}
export function scenesView(
  generation: number,
  start: number,
  end: number,
  position: number,
): Promise<SceneView> {
  return invoke('scenes_view', { generation, start, end, position });
}

export interface ProjectOptions {
  waveformStart: number;
  waveformSpan: number;
  followPlayback: boolean;
  followSubtitle: boolean;
  showScenes: boolean;
  timing?: TimingSettings;
}
interface MediaIdentity {
  size: number;
  modified: string;
  sampleHash: string;
}
export interface SavedScenes {
  version: number;
  mode: SceneMode;
  cuts: number[];
  identity: MediaIdentity;
}
export interface ProjectData {
  version: number;
  mediaPath: string;
  subtitlePath: string | null;
  mediaIdentity: MediaIdentity;
  positionSeconds: number;
  options: ProjectOptions;
  scenes: SavedScenes | null;
}
export interface ProjectAttachment {
  path: string;
  token: number;
}
export interface OpenedProject extends ProjectData {
  token: number;
}
export function openProject(path: string): Promise<OpenedProject> {
  return invoke('project_open', { path });
}
export function attachProject(token: number): Promise<ProjectAttachment> {
  return invoke('project_attach', { token });
}
export function detachProject(): Promise<void> {
  return invoke('project_detach');
}
export function saveProject(request: {
  path: string;
  token: number | null;
  mediaPath: string;
  subtitlePath: string | null;
  session: number;
  positionSeconds: number;
  options: ProjectOptions;
}): Promise<ProjectAttachment> {
  return invoke('project_save', { request });
}
export function restoreScenes(
  path: string,
  session: number,
  saved: SavedScenes,
): Promise<number> {
  return invoke('scenes_restore', { path, session, saved });
}

export function importTimingPreset(
  path: string,
): Promise<ImportedTimingPreset> {
  return commands.timingPresetOpen(path);
}
export async function exportTimingPreset(
  path: string,
  preset: TimingPreset,
): Promise<void> {
  await commands.timingPresetSave(path, preset);
}

export interface Preferences {
  version: 1;
  autoScanScenes: boolean;
  sceneDetail: SceneDetail;
  timing: TimingSettings;
}
export const defaultPreferences: Preferences = {
  version: 1,
  autoScanScenes: true,
  sceneDetail: 'standard',
  timing: defaultTiming,
};
export function getPreferences(): Promise<Preferences> {
  return invoke('preferences_get');
}
export function savePreferences(preferences: Preferences): Promise<void> {
  return invoke('preferences_save', { preferences });
}
