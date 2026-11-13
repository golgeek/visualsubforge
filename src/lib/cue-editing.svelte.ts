import type { DocumentSession } from './document-session.svelte';
import type { MediaSession } from './media-session.svelte';
import type { TimelineView } from './timeline-view.svelte';
import {
  cueOperationLabels,
  planCueOperation,
  type CueOperation,
} from './cue-operations';
export class CueEditing {
  constructor(
    private subtitles: DocumentSession,
    private media: MediaSession,
    private timeline: TimelineView,
  ) {}
  context = $derived.by(() => {
    const doc = this.subtitles.snapshot;
    const selected = this.subtitles.selected;
    const draftDirty = this.subtitles.dirty;
    const status = this.media.status;
    return {
      timeStepMs: doc.format?.timeStepMs ?? 1,
      cues: doc.cues,
      selected,
      loaded: doc.loaded,
      draftDirty,
      positionSeconds:
        status.loaded && !status.seeking && !this.media.pending
          ? status.positionSeconds
          : null,
      range: this.timeline.range,
    };
  });
  operations = $derived.by(() => {
    const context = this.context;
    return (Object.entries(cueOperationLabels) as [CueOperation, string][]).map(
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
    );
  });
}
