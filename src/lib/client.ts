import { commands, events } from './generated/ipc';
import type * as IPC from './generated/ipc';
import { isTauri } from '@tauri-apps/api/core';
import { defaultTiming } from './magnetic-timing';
import { builtinQualityProfiles } from './generated/quality-defaults';

export type DocumentAction = IPC.visualsubforge_document_Action_Deserialize;
export type DocumentSnapshot = IPC.visualsubforge_document_Snapshot_Serialize;
export type Cue = IPC.visualsubforge_document_CueView_Serialize;
export type RecoveryStatus =
  IPC.visualsubforge_document_RecoveryStatus_Serialize;
export type TranslationAction = Parameters<
  typeof commands.translationAction
>[0];
export type TranslationReply = Awaited<
  ReturnType<typeof commands.translationAction>
>;
export type TranslationKey = TranslationReply['key'];
export const translationAction = (action: TranslationAction) =>
  commands.translationAction(action);
export type TimingPreset = IPC.visualsubforge_timing_preset_Preset;
export type TimingSettings = IPC.visualsubforge_timing_preset_Settings;
export type MediaAction = IPC.visualsubforge_media_Action;
export type MediaStatus = IPC.visualsubforge_media_Status_Serialize;
export type WorkspaceAction = IPC.visualsubforge_desktop_WorkspaceAction;
export type FileChoice = IPC.visualsubforge_desktop_FileChoice;
export type MenuState = IPC.visualsubforge_desktop_MenuState;
export type WaveformView = IPC.visualsubforge_waveform_WaveformView_Serialize;
export type SceneMode = IPC.visualsubforge_scenes_Mode;
export type SceneView = IPC.visualsubforge_scenes_SceneView_Serialize;
export type ResyncView = Awaited<ReturnType<typeof commands.resyncView>>;
export type GapView = Awaited<ReturnType<typeof commands.gapView>>;
export type SceneRepairView = Awaited<
  ReturnType<typeof commands.sceneRepairView>
>;
export type DurationRepairView = Awaited<
  ReturnType<typeof commands.durationRepairView>
>;

export type StatisticsView = Awaited<
  ReturnType<typeof commands.statisticsView>
>;
export type StatisticsSelection = Parameters<
  typeof commands.statisticsStart
>[3];
export type StatisticsSummary = NonNullable<StatisticsView['summary']>;
export const statisticsStart = (
  session: number,
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
  selection: StatisticsSelection,
) =>
  commands.statisticsStart(
    session,
    document.generation,
    document.revision,
    selection,
  );
export const statisticsView = (id: number, offset = 0, limit = 100) =>
  commands.statisticsView(id, offset, limit);
export async function statisticsCancel(id: number): Promise<void> {
  await commands.statisticsCancel(id);
}

export type StyleForm = Awaited<ReturnType<typeof commands.styleForm>>;
export type StyleReview = Awaited<ReturnType<typeof commands.styleView>>;
export type StyleOperation = Parameters<typeof commands.styleStart>[2];
export const styleForm = (
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
  name: string | null,
) => commands.styleForm(document.generation, document.revision, name);
export const styleStart = (
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
  operation: StyleOperation,
) => commands.styleStart(document.generation, document.revision, operation);
export const styleView = (id: number) => commands.styleView(id);
export async function styleCancel(id: number): Promise<void> {
  await commands.styleCancel(id);
}
export const styleAccept = (id: number) => commands.styleAccept(id);

export type FontReview = Awaited<ReturnType<typeof commands.fontView>>;
export type FontOperation = Parameters<typeof commands.fontStart>[2];
export type FontInventory = NonNullable<FontReview['inventory']>;
export type FontEntry = FontInventory['entries'][number];
export const fontStart = (
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
  operation: FontOperation,
) => commands.fontStart(document.generation, document.revision, operation);
export const fontView = (id: number) => commands.fontView(id);
export async function fontCancel(id: number): Promise<void> {
  await commands.fontCancel(id);
}
export const fontAccept = (id: number) => commands.fontAccept(id);

export type ConversionReview = Awaited<
  ReturnType<typeof commands.conversionView>
>;
export type ConversionOptions = Parameters<typeof commands.conversionStart>[2];
export const conversionStart = (
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
  options: ConversionOptions,
) => commands.conversionStart(document.generation, document.revision, options);
export const conversionView = (id: number) => commands.conversionView(id);
export async function conversionCancel(id: number): Promise<void> {
  await commands.conversionCancel(id);
}
export async function conversionPublish(
  id: number,
  path: string,
): Promise<void> {
  await commands.conversionPublish(id, path);
}

export type TextView = Awaited<ReturnType<typeof commands.textView>>;
export type TextOperation = Parameters<typeof commands.textStart>[2];
export type TextTarget = Parameters<typeof commands.textAccept>[1];
export type TextFormat = Parameters<typeof commands.textFormat>[3];
export type TextFormattingContext = Parameters<typeof commands.textFormat>[4];

export const textStart = (
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
  operation: TextOperation,
) => commands.textStart(document.generation, document.revision, operation);
export const textView = (id: number, offset = 0, limit = 100) =>
  commands.textView(id, offset, limit);
export const textMatches = (
  id: number,
  cueIndex: number,
  offset = 0,
  limit = 100,
) => commands.textMatches(id, cueIndex, offset, limit);
export async function textCancel(id: number): Promise<void> {
  await commands.textCancel(id);
}
export const textAccept = (id: number, target: TextTarget = null) =>
  commands.textAccept(id, target);
export const textFormat = (
  text: string,
  start: number,
  end: number,
  style: TextFormat,
  context: TextFormattingContext = null,
) => commands.textFormat(text, start, end, style, context);
export type ProjectOptions = IPC.visualsubforge_project_Options_Deserialize;
export type LegacyImportReview = Awaited<
  ReturnType<typeof commands.projectImportView>
>;
export type LegacyImportRequest = Parameters<
  typeof commands.projectImportStart
>[0];
export const projectImportStart = (request: LegacyImportRequest) =>
  commands.projectImportStart(request);
export const projectImportView = (id: number) => commands.projectImportView(id);
export async function projectImportCancel(id: number): Promise<void> {
  await commands.projectImportCancel(id);
}
export async function projectImportPublish(
  id: number,
  path: string,
): Promise<void> {
  await commands.projectImportPublish(id, path);
}
export type ProjectData = IPC.visualsubforge_project_Project_Serialize;
export type SavedScenes = IPC.visualsubforge_scenes_Saved_Deserialize;
export type OpenedProject = IPC.visualsubforge_project_store_Opened_Serialize;
export type ProjectAttachment =
  IPC.visualsubforge_project_store_Attachment_Serialize;
export type SaveProjectRequest =
  IPC.visualsubforge_project_SaveRequest_Deserialize;
export type Preferences = IPC.visualsubforge_preferences_Preferences_Serialize;
export type QualityProfile =
  IPC.vsf_application_quality_profile_Profile_Serialize;
export type QualityView = IPC.visualsubforge_quality_check_View_Serialize;
export type QualityRequest = IPC.visualsubforge_quality_check_Request;
export type QualityMetric = QualityView['metrics'][number];
export type QualityDraft = IPC.visualsubforge_quality_check_Draft;
export type QualityRule = NonNullable<QualityRequest['rule']>;
export type QualitySettingsData = Awaited<
  ReturnType<typeof commands.qualitySettings>
>;
export type SceneDetail = IPC.media_analysis_scenes_Detail;
type ImportedTimingPreset = IPC.visualsubforge_timing_preset_Imported;
type TraceInput = IPC.visualsubforge_workflow_trace_InputEvent_Deserialize;

// Read once; diagnostics never delay a document request or alter its result.
// The native process explicitly opts in with VSF_WORKFLOW_TRACE=1.
let traceWorkflow = false;
if (isTauri()) {
  void commands
    .workflowTraceEnabled()
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
  const sample: TraceInput = {
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
    void commands.workflowTraceInput(sample).catch(() => {});
  });
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
  subtitleFontDiagnostics: [],
  previewDocumentGeneration: null,
  previewDocumentRevision: null,
};
// Browser preview stays inert; production calls use the generated JSON boundary.
export async function mediaAction(
  action: MediaAction,
  session: number,
): Promise<MediaStatus> {
  if (!isTauri()) return emptyStatus;
  return commands.mediaAction(action, session);
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
export async function documentAction(
  action: DocumentAction,
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
): Promise<DocumentSnapshot> {
  const started = traceWorkflow ? performance.now() : null;
  const startedUnixMs = traceWorkflow ? Date.now() : null;
  const result = await commands.documentAction(
    action,
    document.generation,
    document.revision,
  );
  if (started !== null && startedUnixMs !== null && action.kind !== 'status') {
    // Measure IPC through response decoding, before optional diagnostic IO.
    // Rendering the accepted UI and native subtitle presentation are separate.
    void commands
      .workflowTraceAcknowledgment({
        operation: action.kind,
        generation: result.generation,
        revision: result.revision,
        elapsedMs: performance.now() - started,
        startedUnixMs,
      })
      .catch(() => {});
  }
  return result;
}
export async function workspaceActions(
  callback: (action: WorkspaceAction) => void,
): Promise<() => void> {
  if (!isTauri()) return () => {};
  return events.workspaceAction.listen((event) => callback(event.payload));
}
export function chooseFile(
  kind: FileChoice,
  current: string | null = null,
): Promise<string | null> {
  return commands.chooseFile(kind, current);
}
export async function updateMenu(state: MenuState): Promise<void> {
  if (isTauri()) await commands.updateMenu(state);
}
export async function finishExit(
  document: DocumentSnapshot,
  discard: boolean,
): Promise<void> {
  await commands.finishExit(document.generation, document.revision, discard);
}
export function waveformStart(
  path: string,
  audioOrdinal: number,
  session: number,
): Promise<number> {
  return commands.waveformStart(path, audioOrdinal, session);
}
export function waveformView(
  generation: number,
  start: number,
  end: number,
  pixels: number,
): Promise<WaveformView> {
  return commands.waveformView(
    generation,
    start,
    end,
    Math.min(4096, Math.max(1, Math.ceil(pixels))),
  );
}

export function recoveryStatus(): Promise<RecoveryStatus> {
  return commands.recoveryStatus();
}
export async function discardRecovery(id: string): Promise<void> {
  await commands.recoveryDiscard(id);
}

export function scenesStart(
  path: string,
  session: number,
  mode: SceneMode,
  detail: SceneDetail = 'standard',
): Promise<number> {
  return commands.scenesStart(path, session, mode, detail);
}
export function scenesCancel(session: number): Promise<number> {
  return commands.scenesCancel(session);
}
export function resyncStart(
  source: string,
  session: number,
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
): Promise<number> {
  return commands.resyncStart(
    source,
    session,
    document.generation,
    document.revision,
  );
}
export function resyncView(
  id: number,
  cueOffset = 0,
  sectionOffset = 0,
): Promise<ResyncView> {
  return commands.resyncView(id, cueOffset, sectionOffset);
}
export async function resyncCancel(id: number): Promise<void> {
  await commands.resyncCancel(id);
}
export function resyncAccept(id: number): Promise<DocumentSnapshot> {
  return commands.resyncAccept(id);
}
export function gapStart(
  session: number,
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
) {
  return commands.gapStart(session, document.generation, document.revision);
}
export function durationRepairStart(
  session: number,
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
  selectedCueId: string | null,
) {
  return commands.durationRepairStart(
    session,
    document.generation,
    document.revision,
    selectedCueId,
  );
}
export function durationRepairView(id: number, offset = 0, limit = 100) {
  return commands.durationRepairView(id, offset, limit);
}
export async function durationRepairCancel(id: number): Promise<void> {
  await commands.durationRepairCancel(id);
}
export function durationRepairAccept(id: number): Promise<DocumentSnapshot> {
  return commands.durationRepairAccept(id);
}
export function sceneRepairStart(
  session: number,
  document: Pick<DocumentSnapshot, 'generation' | 'revision'>,
) {
  return commands.sceneRepairStart(
    session,
    document.generation,
    document.revision,
  );
}
export function sceneRepairView(id: number, offset = 0, limit = 100) {
  return commands.sceneRepairView(id, offset, limit);
}
export async function sceneRepairCancel(id: number): Promise<void> {
  await commands.sceneRepairCancel(id);
}
export function sceneRepairAccept(id: number): Promise<DocumentSnapshot> {
  return commands.sceneRepairAccept(id);
}
export function gapView(id: number, offset = 0, limit = 100) {
  return commands.gapView(id, offset, limit);
}
export async function gapCancel(id: number): Promise<void> {
  await commands.gapCancel(id);
}
export function gapAccept(
  id: number,
  acknowledgeNoMedia: boolean,
): Promise<DocumentSnapshot> {
  return commands.gapAccept(id, acknowledgeNoMedia);
}
export function scenesView(
  generation: number,
  start: number,
  end: number,
  position: number,
): Promise<SceneView> {
  return commands.scenesView(generation, start, end, position);
}

export type ProjectReferenceOverride = NonNullable<
  Parameters<typeof commands.projectOpen>[1]
>;
export type ProjectReferenceIssue = NonNullable<
  OpenedProject['referenceIssue']
>;
export function openProject(
  path: string,
  reference: ProjectReferenceOverride | null = null,
): Promise<OpenedProject> {
  return commands.projectOpen(path, reference);
}
export function attachProject(
  token: number,
  session: number,
  document: Pick<DocumentSnapshot, 'generation' | 'revision' | 'translation'>,
): Promise<ProjectAttachment> {
  return commands.projectAttach(
    token,
    session,
    document.generation,
    document.revision,
    document.translation?.epoch ?? 0,
  );
}
export async function detachProject(): Promise<void> {
  await commands.projectDetach();
}
export function saveProject(
  request: SaveProjectRequest,
): Promise<ProjectAttachment> {
  return commands.projectSave(request);
}
export function restoreScenes(
  path: string,
  session: number,
  saved: SavedScenes,
): Promise<number> {
  return commands.scenesRestore(path, session, saved);
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

export const defaultPreferences: Preferences = {
  version: 1,
  autoScanScenes: true,
  sceneDetail: 'standard',
  timing: defaultTiming,
  quality: builtinQualityProfiles[0]!,
  showQualityMetrics: false,
  personalWords: { englishUs: [], french: [] },
};
export function getPreferences(): Promise<Preferences> {
  return commands.preferencesGet();
}
export async function savePreferences(
  preferences: Preferences,
  epoch: number,
): Promise<void> {
  await commands.preferencesSave(preferences, epoch);
}
export function getQualitySettings(): Promise<QualitySettingsData> {
  return commands.qualitySettings();
}
export type LanguageWords = Awaited<ReturnType<typeof commands.languageWords>>;
export type LanguageWordChange = Parameters<
  typeof commands.languageWordsChange
>[1];
export type LanguageNotices =
  ReturnType<typeof commands.languageNotices> extends Promise<infer T>
    ? T
    : never;
export const getLanguageWords = () => commands.languageWords();
export const changeLanguageWords = (
  epoch: number,
  change: LanguageWordChange,
) => commands.languageWordsChange(epoch, change);
export const getLanguageNotices = () => commands.languageNotices();
export function readQuality(request: QualityRequest): Promise<QualityView> {
  return commands.qualityRead(request);
}
export async function cancelQuality(id: number): Promise<void> {
  await commands.qualityCancel(id);
}
export function importQualityProfile(path: string): Promise<QualityProfile> {
  return commands.qualityProfileOpen(path);
}
export async function exportQualityProfile(
  path: string,
  profile: QualityProfile,
): Promise<void> {
  await commands.qualityProfileSave(path, profile);
}

export type BatchTimingView = Awaited<
  ReturnType<typeof commands.batchTimingView>
>;
export type BatchTimingRequest = Parameters<
  typeof commands.batchTimingStart
>[3];
export function batchTimingStart(
  session: number,
  document: DocumentSnapshot,
  request: BatchTimingRequest,
) {
  return commands.batchTimingStart(
    session,
    document.generation,
    document.revision,
    request,
  );
}
export function batchTimingView(id: number, offset = 0, limit = 100) {
  return commands.batchTimingView(id, offset, limit);
}
export async function batchTimingCancel(id: number): Promise<void> {
  await commands.batchTimingCancel(id);
}
export function batchTimingAccept(id: number): Promise<DocumentSnapshot> {
  return commands.batchTimingAccept(id);
}
