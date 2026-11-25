import type { Cue, DocumentAction } from '../client';

export type CueOperation =
  'createCue' | 'deleteCue' | 'splitCue' | 'mergePrevious' | 'mergeNext';
export const cueOperationLabels: Record<CueOperation, string> = {
  createCue: 'Create cue',
  deleteCue: 'Delete cue',
  splitCue: 'Split at playhead',
  mergePrevious: 'Merge with previous',
  mergeNext: 'Merge with next',
};
export interface OperationContext {
  cues: readonly Cue[];
  selected: number | null;
  loaded: boolean;
  draftDirty: boolean;
  positionSeconds: number | null;
  range: readonly [number, number] | null;
  timeStepMs?: number;
}
export interface CueOperationPlan {
  action: DocumentAction;
  selection: number | null | 'created';
}
const maxMilliseconds = 9_999 * 3_600_000 + 3_599_999;
function milliseconds(seconds: number, step: number): number {
  const value = Math.round((seconds * 1000) / step) * step;
  if (!Number.isFinite(seconds) || seconds < 0 || value > maxMilliseconds)
    throw new Error('Cue time must be between zero and 9999:59:59,999.');
  return value;
}

// Pure planning keeps menu, controls and shortcut requests on the same policy.
// This is an interaction preview; Rust revalidates the revision and operation.
export function planCueOperation(
  operation: CueOperation,
  context: OperationContext,
): CueOperationPlan {
  const { cues, selected, loaded, draftDirty, positionSeconds, range } =
    context;
  if (!loaded && operation !== 'createCue')
    throw new Error('Open subtitles or create a cue first.');
  if (draftDirty)
    throw new Error('Apply or revert the current draft before changing cues.');
  const index = cues.findIndex((cue) => cue.id === selected);
  const step = context.timeStepMs ?? 1;
  const cue = cues[index];
  if (operation === 'createCue') {
    const start = range?.[0] ?? positionSeconds ?? (cue?.endMs ?? 0) / 1000;
    const startMs = milliseconds(start, step);
    const endMs = milliseconds(range?.[1] ?? start + 2, step);
    if (endMs <= startMs)
      throw new Error(
        `The new cue needs a range of at least ${step} millisecond${step === 1 ? '' : 's'}.`,
      );
    return {
      action: {
        kind: 'create',
        afterId: cue?.id ?? null,
        startMs,
        endMs,
        text: '',
      },
      selection: 'created',
    };
  }
  if (!cue) throw new Error('Select a cue first.');
  if (operation === 'deleteCue')
    return {
      action: { kind: 'delete', cueId: cue.id },
      selection: cues[index + 1]?.id ?? cues[index - 1]?.id ?? null,
    };
  if (operation === 'splitCue') {
    if (positionSeconds === null)
      throw new Error('Wait for a settled media playhead before splitting.');
    const atMs = milliseconds(positionSeconds, step);
    if (atMs <= cue.startMs || atMs >= cue.endMs)
      throw new Error('Move the playhead strictly inside the selected cue.');
    return {
      action: { kind: 'split', cueId: cue.id, atMs },
      selection: 'created',
    };
  }
  const first = operation === 'mergePrevious' ? cues[index - 1] : cue;
  const second = operation === 'mergePrevious' ? cue : cues[index + 1];
  if (!first || !second)
    throw new Error('There is no cue on that side to merge.');
  return {
    action: { kind: 'merge', firstId: first.id, secondId: second.id },
    selection: first.id,
  };
}

export function reconcileCueSelection(
  before: readonly Cue[],
  after: readonly Cue[],
  selected: number | null,
): number | null {
  if (after.some((cue) => cue.id === selected)) return selected;
  const index = Math.max(
    0,
    before.findIndex((cue) => cue.id === selected),
  );
  return after[Math.min(index, after.length - 1)]?.id ?? null;
}

export function operationSelection(
  plan: CueOperationPlan,
  before: readonly Cue[],
  after: readonly Cue[],
): number | null {
  if (plan.selection !== 'created') return plan.selection;
  const ids = new Set(before.map((cue) => cue.id));
  return after.find((cue) => !ids.has(cue.id))?.id ?? null;
}

export type GroupOperation = 'mergeRun' | 'mergeAsDialog' | 'deleteMany';

export function planGroupOperation(
  operation: GroupOperation,
  ids: readonly number[],
  cues: readonly Cue[],
): CueOperationPlan {
  if (operation === 'mergeAsDialog' && ids.length !== 2)
    throw new Error('Merge as dialog needs exactly two cues');
  if (ids.length < 2) throw new Error('Select at least two cues');
  const selected = new Set(ids);
  const indices: number[] = [];
  for (const [index, cue] of cues.entries())
    if (selected.has(cue.id)) indices.push(index);
  if (indices.length !== ids.length)
    throw new Error('The selected cues changed');
  const first = indices[0]!;
  const ordered = indices.map((index) => cues[index]!.id);
  if (operation === 'deleteMany') {
    const next = cues.slice(first).find((cue) => !selected.has(cue.id));
    return {
      action: { kind: 'deleteMany', ids: ordered },
      selection: next?.id ?? cues[first - 1]?.id ?? null,
    };
  }
  for (let i = 1; i < indices.length; i++) {
    const before = indices[i - 1]!,
      after = indices[i]!;
    if (after !== before + 1)
      throw new Error(`Cues ${before + 1} and ${after + 1} are not adjacent`);
    if (
      cues[before]!.style !== cues[after]!.style ||
      cues[before]!.actor !== cues[after]!.actor
    )
      throw new Error('Merge requires matching cue style and actor');
  }
  return {
    action: {
      kind: 'mergeRun',
      ids: ordered,
      dialogue: operation === 'mergeAsDialog',
    },
    selection: ordered[0]!,
  };
}

export function groupOperationReason(
  operation: GroupOperation,
  ids: readonly number[],
  cues: readonly Cue[],
): string | null {
  try {
    planGroupOperation(operation, ids, cues);
    return null;
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
}
