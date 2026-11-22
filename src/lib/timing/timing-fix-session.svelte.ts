import {
  timingFixStart,
  timingFixView,
  timingFixCancel,
  type TimingFixInput,
  type TimingFixView,
} from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
import {
  TimingRepairSession,
  type LiveTimingReview,
} from './timing-repair-session.svelte';

const backend = { timingFixStart, timingFixView, timingFixCancel };
export class TimingFixSession extends TimingRepairSession<
  TimingFixView,
  TimingFixInput
> {
  constructor(
    subtitles: DocumentSession,
    mediaSession: () => number,
    input: () => TimingFixInput,
    live?: LiveTimingReview,
    api = backend,
  ) {
    super(
      subtitles,
      mediaSession,
      {
        start: api.timingFixStart,
        view: (id, offset, limit, ids) =>
          api.timingFixView(id, offset, limit, 'changes', ids),
        cancel: api.timingFixCancel,
        accept: (id, doc) => subtitles.acceptTimingFix(id, doc),
      },
      input,
      ['checked', 'reduced', 'notRequired', 'noMedia'],
      (a, b) => JSON.stringify(a) === JSON.stringify(b),
      live,
    );
  }
}
