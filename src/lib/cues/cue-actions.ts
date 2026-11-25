import { tick } from 'svelte';
import { textFormat, type TextFormat } from '../client';
import type { TimelineView } from '../timeline/timeline-view.svelte';
import type { Workspace } from '../workspace/workspace.svelte';
import type { DocumentSession } from '../document/document-session.svelte';
import type { TimingGesture } from '../timing/timing-gesture.svelte';
import { traceApplyInput } from '../client';
import {
  planCueOperation,
  planGroupOperation,
  type GroupOperation,
  cueOperationLabels,
} from './cue-operations';

export type CueAction =
  | 'createCue'
  | 'deleteCue'
  | 'splitCue'
  | 'mergePrevious'
  | 'mergeNext'
  | 'mergeAsDialog'
  | 'apply'
  | 'undo'
  | 'redo';
interface Owners {
  workspace: Workspace;
  subtitles: DocumentSession;
  gestures: TimingGesture;
  timeline: Pick<TimelineView, 'selectRange'>;
  operationContext: Parameters<typeof planCueOperation>[1];
  suspendFollowing: () => void;
  revealSelection: () => Promise<void>;
  focusText: () => Promise<void>;
}

export async function commitCueGroup(
  operation: GroupOperation,
  ids: readonly number[],
  subtitles: DocumentSession,
) {
  const plan = planGroupOperation(operation, ids, subtitles.snapshot.cues);
  return subtitles.command(plan.action, { plan });
}

export async function runCueAction(
  action: CueAction,
  owners: Owners,
  event?: Event,
) {
  const {
    workspace,
    subtitles,
    gestures,
    suspendFollowing,
    revealSelection,
    focusText,
  } = owners;
  gestures.cancelHeld();
  if (action === 'apply' && event && subtitles.cue && subtitles.dirty)
    traceApplyInput(event, subtitles.snapshot);
  // Native menu/keyboard history stays local to text inputs. Explicit buttons
  // use accepted document history even when the editor retains focus.
  const focused = document.activeElement;
  if (
    !event &&
    (action === 'undo' || action === 'redo') &&
    (focused instanceof HTMLInputElement ||
      focused instanceof HTMLTextAreaElement)
  ) {
    document.execCommand(action);
    return;
  }
  workspace.notice = '';
  await workspace.task(async () => {
    switch (action) {
      case 'mergeAsDialog': {
        const ids = [...subtitles.selectedIds];
        if (!(await subtitles.apply())) return;
        suspendFollowing();
        if (await commitCueGroup(action, ids, subtitles))
          await revealSelection();
        break;
      }
      case 'createCue':
      case 'deleteCue':
      case 'splitCue':
      case 'mergePrevious':
      case 'mergeNext': {
        if (!(await subtitles.apply())) return;
        const plan = planCueOperation(action, owners.operationContext);
        suspendFollowing();
        await subtitles.command(plan.action, { plan });
        await revealSelection();
        if (action === 'createCue') await focusText();
        workspace.notice = `${cueOperationLabels[action]} applied`;
        break;
      }
      case 'apply':
        await subtitles.apply();
        workspace.notice = 'Edit applied';
        break;
      case 'undo':
      case 'redo':
        if (!(await subtitles.apply())) return;
        if (subtitles.snapshot.loaded) {
          suspendFollowing();
          await subtitles.command({ kind: action });
          // History is authoritative in Rust. Rebuild the visible selection from
          // the returned snapshot, never from the gesture's former preview.
          const cue = subtitles.cue;
          if (cue)
            owners.timeline.selectRange([cue.startMs / 1000, cue.endMs / 1000]);
          await revealSelection();
        }
        break;
    }
  });
}

export async function formatCueText(
  owners: {
    workspace: Workspace;
    subtitles: DocumentSession;
    alive(): boolean;
    editor(): HTMLTextAreaElement | undefined;
  },
  style: TextFormat,
) {
  const { workspace, subtitles } = owners;
  const cue = subtitles.cue;
  const textEditor = owners.editor();
  if (workspace.busy || workspace.confirming || !cue || !textEditor) return;
  const editor = textEditor;
  const before = subtitles.text;
  const beforeStyle = subtitles.style;
  const generation = subtitles.snapshot.generation;
  const id = subtitles.selected;
  const start = textEditor.selectionStart;
  const end = textEditor.selectionEnd;
  await workspace.task(async () => {
    if (!(await subtitles.apply())) return;
    if (
      !owners.alive() ||
      subtitles.snapshot.generation !== generation ||
      subtitles.selected !== id ||
      subtitles.text !== before ||
      subtitles.style !== beforeStyle
    )
      return;
    const snapshot = subtitles.snapshot;
    const result = await textFormat(before, start, end, style, {
      generation: snapshot.generation,
      revision: snapshot.revision,
      cueId: cue.id,
      styleName: beforeStyle,
    });
    if (
      !owners.alive() ||
      subtitles.snapshot !== snapshot ||
      subtitles.selected !== id ||
      subtitles.style !== beforeStyle ||
      subtitles.text !== before
    )
      return;
    subtitles.text = result.text;
    // Formatting is an explicit command: seal the typing group and give the
    // formatted result its own Rust history entry.
    if (!(await subtitles.apply(null)) || !owners.alive()) return;
    await tick();
    editor.focus();
    editor.setSelectionRange(result.start, result.end);
    workspace.notice = 'Formatting applied';
  });
}
