import { gapStart, gapView, gapCancel, type GapView } from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
import { TimingRepairSession } from './timing-repair-session.svelte';
const client = { gapStart, gapView, gapCancel };

// Gap review alone permits explicitly acknowledged missing-media coverage.
// The shared owner handles requests; Rust still validates the complete proposal.
export class GapSession extends TimingRepairSession<GapView> {
  acknowledgeNoMedia = $state(false);
  constructor(
    subtitles: DocumentSession,
    mediaSession: () => number,
    api = client,
  ) {
    super(
      subtitles,
      mediaSession,
      {
        start: async (session, document) => {
          this.acknowledgeNoMedia = false;
          return api.gapStart(session, document);
        },
        view: api.gapView,
        cancel: api.gapCancel,
        accept: (id, document) =>
          subtitles.acceptGap(id, document, this.acknowledgeNoMedia),
      },
      () => null,
      [],
    );
  }
  protected override eligible(view: GapView) {
    return view.coverage !== 'noMedia' || this.acknowledgeNoMedia;
  }
}
