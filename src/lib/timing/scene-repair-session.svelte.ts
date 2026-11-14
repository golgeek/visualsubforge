import {
  sceneRepairStart,
  sceneRepairView,
  sceneRepairCancel,
} from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
import {
  TimingRepairSession,
  type LiveTimingReview,
} from './timing-repair-session.svelte';
const client = { sceneRepairStart, sceneRepairView, sceneRepairCancel };
export class SceneRepairSession extends TimingRepairSession<
  Awaited<ReturnType<typeof sceneRepairView>>
> {
  constructor(
    subtitles: DocumentSession,
    mediaSession: () => number,
    api = client,
    live?: LiveTimingReview,
  ) {
    super(
      subtitles,
      mediaSession,
      {
        start: (session, document, _scope, selection) =>
          api.sceneRepairStart(session, document, selection),
        view: api.sceneRepairView,
        cancel: api.sceneRepairCancel,
        accept: (id, document) => subtitles.acceptSceneRepair(id, document),
      },
      () => null,
      ['checked', 'reduced'],
      Object.is,
      live,
    );
  }
}
