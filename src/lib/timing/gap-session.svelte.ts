import { gapStart, gapView, gapCancel, type GapView } from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
import {
  TimingRepairSession,
  type LiveTimingReview,
} from './timing-repair-session.svelte';
const client = { gapStart, gapView, gapCancel };

// Explicit gap acceptance permits missing-media coverage. The dock presents a
// note beside Apply; the legacy standalone review retains its checkbox.
// The shared owner handles requests; Rust still validates the complete proposal.
export class GapSession extends TimingRepairSession<GapView> {
  acknowledgeNoMedia = $state(false);
  constructor(
    subtitles: DocumentSession,
    mediaSession: () => number,
    api = client,
    private liveReview?: LiveTimingReview,
  ) {
    super(
      subtitles,
      mediaSession,
      {
        start: async (session, document, _scope, selection) => {
          this.acknowledgeNoMedia = false;
          return api.gapStart(session, document, selection);
        },
        view: api.gapView,
        cancel: api.gapCancel,
        accept: (id, document) =>
          subtitles.acceptGap(
            id,
            document,
            !!liveReview || this.acknowledgeNoMedia,
          ),
      },
      () => null,
      [],
      Object.is,
      liveReview,
    );
  }
  protected override eligible(view: GapView) {
    return (
      !!this.liveReview ||
      view.coverage !== 'noMedia' ||
      this.acknowledgeNoMedia
    );
  }
}
