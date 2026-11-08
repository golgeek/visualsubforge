import type { Cue, DocumentAction, DocumentSnapshot } from './client';

export type CueEdit = Extract<DocumentAction, { kind: 'edit' | 'editStyled' }>;

/** Rebase only fields changed by this draft. Rust still validates and commits. */
export function retryEdit(
  base: DocumentSnapshot,
  current: DocumentSnapshot,
  edit: CueEdit,
): CueEdit | null {
  if (!current.loaded || current.generation !== base.generation) return null;
  const before = base.cues.find((cue) => cue.id === edit.cueId);
  const after = current.cues.find((cue) => cue.id === edit.cueId);
  if (!before || !after) return null;
  const fields = ['startMs', 'endMs', 'text'] as const;
  for (const field of fields)
    if (edit[field] !== before[field] && after[field] !== before[field])
      return null;
  if (
    edit.kind === 'editStyled' &&
    edit.style !== (before.style ?? '') &&
    after.style !== before.style
  )
    return null;
  const value = <K extends keyof Pick<Cue, 'startMs' | 'endMs' | 'text'>>(
    field: K,
  ) => (edit[field] === before[field] ? after[field] : edit[field]);
  const merged = {
    ...edit,
    startMs: value('startMs'),
    endMs: value('endMs'),
    text: value('text'),
  };
  return edit.kind === 'editStyled'
    ? {
        ...merged,
        kind: 'editStyled',
        style:
          edit.style === (before.style ?? '')
            ? (after.style ?? '')
            : edit.style,
      }
    : merged;
}
