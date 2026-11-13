import { tick } from 'svelte';
import { textFormat, type TextFormat } from './client';
import type { Workspace } from './workspace.svelte';
import type { DocumentSession } from './document-session.svelte';
import type { TimingGesture } from './timing-gesture.svelte';
import { traceApplyInput } from './client';
import { planCueOperation, cueOperationLabels } from './cue-operations';

export type CueAction =
  | 'createCue'
  | 'deleteCue'
  | 'splitCue'
  | 'mergePrevious'
  | 'mergeNext'
  | 'apply'
  | 'undo'
  | 'redo';
interface Owners {
  workspace: Workspace;
  subtitles: DocumentSession;
  gestures: TimingGesture;
  operationContext: Parameters<typeof planCueOperation>[1];
  suspendFollowing: () => void;
  revealSelection: () => Promise<void>;
  focusText: () => Promise<void>;
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
      case 'createCue':
      case 'deleteCue':
      case 'splitCue':
      case 'mergePrevious':
      case 'mergeNext': {
        if (action === 'createCue') await subtitles.apply();
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
        if (subtitles.dirty)
          throw new Error(
            'Apply or revert the current draft before undoing an accepted edit.',
          );
        if (subtitles.snapshot.loaded) {
          suspendFollowing();
          await subtitles.command({ kind: action });
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
  const snapshot = subtitles.snapshot;
  const id = subtitles.selected;
  const start = textEditor.selectionStart;
  const end = textEditor.selectionEnd;
  await workspace.task(async () => {
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
    await tick();
    editor.focus();
    editor.setSelectionRange(result.start, result.end);
    workspace.notice =
      'Formatting updated in the cue draft. Apply edit to accept it.';
  });
}
