import type { DocumentSession } from './document-session.svelte';
import type { MediaSession } from './media-session.svelte';
import type { TimelineView } from './timeline-view.svelte';
import {
  cueOperationLabels,
  planCueOperation,
  type CueOperation,
} from './cue-operations';
export function createCueEditing(
  subtitles: DocumentSession,
  media: MediaSession,
  timeline: TimelineView,
) {
  const doc = $derived(subtitles.snapshot);
  const selected = $derived(subtitles.selected);
  const draftDirty = $derived(subtitles.dirty);
  const status = $derived(media.status);
  const context = $derived({
    timeStepMs: doc.format?.timeStepMs ?? 1,
    cues: doc.cues,
    selected,
    loaded: doc.loaded,
    draftDirty,
    positionSeconds:
      status.loaded && !status.seeking && !media.pending
        ? status.positionSeconds
        : null,
    range: timeline.range,
  });
  const cueOperations = $derived(
    (Object.entries(cueOperationLabels) as [CueOperation, string][]).map(
      ([operation, label]) => {
        try {
          return {
            operation,
            label,
            plan: planCueOperation(
              operation,
              operation === 'createCue'
                ? { ...context, draftDirty: false }
                : context,
            ),
            reason: '',
          };
        } catch (e) {
          return {
            operation,
            label,
            plan: null,
            reason: String(e).replace(/^Error: /, ''),
          };
        }
      },
    ),
  );

  return {
    get context() {
      return context;
    },
    get operations() {
      return cueOperations;
    },
  };
}
