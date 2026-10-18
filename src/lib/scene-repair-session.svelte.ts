import { sceneRepairStart, sceneRepairView, sceneRepairCancel } from './client';
import type { DocumentSession } from './document-session.svelte';
import { TimingRepairSession } from './timing-repair-session.svelte';
const client = { sceneRepairStart, sceneRepairView, sceneRepairCancel };
export class SceneRepairSession extends TimingRepairSession<
  Awaited<ReturnType<typeof sceneRepairView>>
> {
  constructor(
    subtitles: DocumentSession,
    mediaSession: () => number,
    api = client,
  ) {
    super(
      subtitles,
      mediaSession,
      {
        start: api.sceneRepairStart,
        view: api.sceneRepairView,
        cancel: api.sceneRepairCancel,
        accept: (id, document) => subtitles.acceptSceneRepair(id, document),
      },
      () => null,
      ['checked', 'reduced'],
    );
  }
}
