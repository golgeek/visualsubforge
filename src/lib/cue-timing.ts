import type { Cue, DocumentAction, DocumentSnapshot } from './client';

export type CueBoundary = 'start' | 'end';
export interface TimingEdit {
  generation: number;
  revision: number;
  mediaSession: number;
  original: Cue;
  preview: Cue;
  timeStepMs?: number;
}
const maximumTime = 9_999 * 3_600_000 + 3_599_999;

export function beginTimingEdit(
  document: DocumentSnapshot,
  selected: number | null,
  mediaSession: number,
  draftDirty: boolean,
): TimingEdit {
  if (draftDirty)
    throw new Error(
      'Apply or revert the current draft before changing timing.',
    );
  const cue = document.cues.find((item) => item.id === selected);
  if (!document.loaded || !cue) throw new Error('Select a cue first.');
  return {
    generation: document.generation,
    revision: document.revision,
    mediaSession,
    original: { ...cue },
    preview: { ...cue },
    timeStepMs: document.format?.timeStepMs ?? 1,
  };
}

// Pointer seconds are converted by the caller once. All preview adjustments use
// integer milliseconds, quantized to the source format's grid. A cue retains at
// least one format tick. Manual typed values are validated by Rust without rounding.
export function previewBoundary(
  edit: TimingEdit,
  boundary: CueBoundary,
  milliseconds: number,
): TimingEdit {
  if (!Number.isSafeInteger(milliseconds))
    throw new Error('Cue boundary must be a finite integer millisecond.');
  const preview = { ...edit.preview };
  const step = edit.timeStepMs ?? 1;
  const rounded = Math.round(milliseconds / step) * step;
  if (boundary === 'start')
    preview.startMs = Math.max(0, Math.min(rounded, preview.endMs - step));
  else
    preview.endMs = Math.max(
      preview.startMs + step,
      Math.min(rounded, Math.floor(maximumTime / step) * step),
    );
  return { ...edit, preview };
}

export function timingAction(
  edit: TimingEdit,
  document: DocumentSnapshot,
  mediaSession: number,
  selected: number | null,
): DocumentAction | null {
  if (
    document.generation !== edit.generation ||
    document.revision !== edit.revision ||
    mediaSession !== edit.mediaSession ||
    selected !== edit.original.id
  )
    throw new Error(
      'Document, selection or media changed; timing gesture cancelled.',
    );
  const { original, preview } = edit;
  if (original.startMs === preview.startMs && original.endMs === preview.endMs)
    return null;
  return {
    kind: 'edit',
    cueId: original.id,
    startMs: preview.startMs,
    endMs: preview.endMs,
    text: original.text,
  };
}
