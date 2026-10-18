import {
  durationRepairStart,
  durationRepairView,
  durationRepairCancel,
} from './client';
import type { DocumentSession } from './document-session.svelte';
import { TimingRepairSession } from './timing-repair-session.svelte';
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
    );
  }
}
