import { TextSession } from '../../../src/lib/text-session.svelte';
import { DocumentSession } from '../../../src/lib/document-session.svelte';
import type { TextView, DocumentSnapshot } from '../../../src/lib/client';
export async function lifecycle(
  sample: TextView,
  original: DocumentSnapshot,
  passed: (m: string) => void,
) {
  const assert = (v: unknown, m: string) => {
    if (!v) throw new Error(m);
  };
  let snapshot = structuredClone(original);
  let releaseAccept!: (v: DocumentSnapshot) => void;
  const subtitles = new DocumentSession({
    chooseFile: async () => null,
    documentAction: async () => snapshot,
    resyncAccept: async () => snapshot,
    gapAccept: async () => snapshot,
    textAccept: () =>
      new Promise((resolve) => {
        releaseAccept = resolve;
      }),
  });
  await subtitles.command({ kind: 'status' });
  let release!: (v: TextView) => void;
  let held: Promise<TextView> | null = null;
  const cancels: number[] = [];
  const session = new TextSession(subtitles, {
    textStart: async () => sample.id,
    textView: async () => held ?? sample,
    textCancel: async (id) => {
      cancels.push(id);
    },
    textMatches: async () => sample.rows[0]!.matches,
  });
  session.mode = 'replace';
  session.query = 'été';
  session.replacement = 'summer';
  await session.start();
  assert(session.canAccept, 'Initial preview unavailable');
  held = new Promise((resolve) => {
    release = resolve;
  });
  const poll = session.poll();
  snapshot = { ...snapshot, revision: snapshot.revision + 1 };
  await subtitles.command({ kind: 'status' });
  release(sample);
  await poll;
  held = null;
  assert(
    session.view === null && !session.canAccept && cancels.includes(sample.id),
    'Stale response revived proposal',
  );
  passed('A held text response cannot overwrite a newer document projection');
  await session.start();
  const accepted = session.accept();
  assert(
    !session.canClose && !session.canStart,
    'Acceptance permits modal close or new request',
  );
  releaseAccept({ ...snapshot, revision: snapshot.revision + 1, dirty: true });
  assert(await accepted, 'Accepted reply not adopted');
  assert(
    subtitles.snapshot.revision === snapshot.revision + 1,
    'Document owner did not reconcile',
  );
  passed(
    'Acceptance protects dialog lifetime until the document owner reconciles the reply',
  );
  session.dispose();
  subtitles.dispose();
}
