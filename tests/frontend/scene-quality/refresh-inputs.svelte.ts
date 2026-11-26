import { tick } from 'svelte';
import type { DocumentSnapshot, TimingFixView } from '../../../src/lib/client';
import { defaultPreferences } from '../../../src/lib/client';
import { DocumentSession } from '../../../src/lib/document/document-session.svelte';
import { TimingFixSession } from '../../../src/lib/timing/timing-fix-session.svelte';

// Exercise the live session's input boundary. Hold the new page so a completed
// read cannot mask a safety gate that was late to react to the change.
export async function checkRefreshInputs(
  original: DocumentSnapshot,
  sample: TimingFixView,
) {
  let document = structuredClone(original);
  const subtitles = new DocumentSession({
    chooseFile: async () => null,
    documentAction: async () => document,
    resyncAccept: async () => {
      throw new Error('Unexpected accept');
    },
  });
  await subtitles.command({ kind: 'status' });
  let media = $state(1);
  let scenes = $state(1);
  let policy = $state(1);
  let tolerance = $state(0);
  let rows = $state([1]);
  let jobs = 0;
  let pages = 0;
  let projections = 0;
  let nextPage: Promise<void> | undefined;
  let session!: TimingFixSession;
  const stop = $effect.root(() => {
    session = new TimingFixSession(
      subtitles,
      () => media,
      () => ({
        options: {
          ...defaultPreferences.timingRepair,
          startToleranceMs: tolerance,
        },
        scope: null,
        families: { gaps: true, speed: true, shots: true },
        toggled: [],
        omitTrades: false,
      }),
      {
        enabled: () => true,
        selection: () => null,
        context: () => scenes + '/' + policy,
        previewIds: () => rows,
        retainDisplay: true,
      },
      {
        timingFixStart: async () => ++jobs,
        timingFixCancel: async () => {},
        timingFixView: async (id, _offset, _limit, _section, ids) => {
          if (ids) projections++;
          else {
            pages++;
            await nextPage;
          }
          return { ...sample, id };
        },
      },
    );
    return () => session.dispose();
  });
  async function until(test: () => boolean) {
    const end = performance.now() + 4000;
    while (!test()) {
      if (performance.now() > end)
        throw new Error('Timed out waiting for preview: ' + session.error);
      await new Promise(requestAnimationFrame);
    }
  }
  async function edit(text: string) {
    document = {
      ...document,
      revision: document.revision + 1,
      cues: document.cues.map((cue, i) => (i === 0 ? { ...cue, text } : cue)),
    };
    await subtitles.command({ kind: 'status' });
  }
  try {
    await until(() => session.canAccept && projections === 1);
    const changes = {
      edit: () => edit('Changed cue'),
      undo: () => edit(original.cues[0]!.text),
      detach: () => {
        media++;
      },
      replaceMedia: () => {
        media++;
      },
      sceneKey: () => {
        scenes++;
      },
      qualityPolicy: () => {
        policy++;
      },
      timingOptions: () => {
        tolerance++;
      },
    };
    for (const [name, change] of Object.entries(changes)) {
      const before = pages;
      let release!: () => void;
      nextPage = new Promise<void>((r) => {
        release = r;
      });
      await change();
      await tick();
      if (session.canAccept) throw new Error(name + ' left Apply enabled');
      await until(() => pages > before);
      if (session.canAccept)
        throw new Error(name + ' enabled Apply during the held read');
      release();
      await until(() => session.canAccept);
      if (pages !== before + 1)
        throw new Error(name + ' read ' + (pages - before) + ' pages');
    }
    const before = pages;
    await until(() => session.preview?.id === jobs);
    const previews = projections;
    rows = [2];
    await until(() => projections > previews);
    if (pages !== before)
      throw new Error('Viewport movement reread the review page');
  } finally {
    stop();
    subtitles.dispose();
  }
}
