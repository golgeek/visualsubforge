import {
  durationRepairStart,
  durationRepairView,
  durationRepairCancel,
} from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
import {
  TimingRepairSession,
  type LiveTimingReview,
} from './timing-repair-session.svelte';
const client = {
  durationRepairStart,
  durationRepairView,
  durationRepairCancel,
};
export class DurationRepairSession extends TimingRepairSession<
  Awaited<ReturnType<typeof durationRepairView>>
> {
  constructor(
    subtitles: DocumentSession,
    mediaSession: () => number,
    scope: () => string | null,
    api = client,
    live?: LiveTimingReview,
  ) {
    super(
      subtitles,
      mediaSession,
      {
        start: api.durationRepairStart,
        view: api.durationRepairView,
        cancel: api.durationRepairCancel,
        accept: (id, document) => subtitles.acceptDurationRepair(id, document),
      },
      scope,
      ['checked', 'reduced', 'notRequired'],
      Object.is,
      live,
    );
  }
}
