import { StyleSession } from '../../../src/lib/style-session.svelte';
import { DocumentSession } from '../../../src/lib/document-session.svelte';
import type {
  StyleForm,
  StyleReview,
  DocumentSnapshot,
} from '../../../src/lib/client';
export async function lifecycle(
  form: StyleForm,
  sample: StyleReview,
  original: DocumentSnapshot,
  passed: (s: string) => void,
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
    styleAccept: () =>
      new Promise((resolve) => {
        releaseAccept = resolve;
      }),
  });
  await subtitles.command({ kind: 'status' });
  let releaseView!: (v: StyleReview) => void;
  let heldView: Promise<StyleReview> | null = null;
  let releaseForm!: (v: StyleForm) => void;
  let heldForm: Promise<StyleForm> | null = null;
  let releaseStart!: (v: number) => void;
  let heldStart: Promise<number> | null = null;
  const cancels: number[] = [];
  const api = {
    styleForm: async () => heldForm ?? structuredClone(form),
    styleStart: async () => heldStart ?? sample.id,
    styleView: async () => heldView ?? structuredClone(sample),
    styleCancel: async (id: number) => {
      cancels.push(id);
    },
  };
  const session = new StyleSession(subtitles, api);
  await session.begin('edit', 'Title');
  session.fields.find((f) => f.name === 'Fontsize')!.value = '42';
  await session.start();
  assert(session.canAccept, 'Review unavailable');
  session.fields.find((f) => f.name === 'Fontsize')!.value = '44';
  assert(!session.canAccept, 'Changed form kept acceptance enabled');
  await session.poll();
  assert(
    !session.view && cancels.includes(sample.id),
    'Changed form did not retire proposal',
  );
  passed(
    'Changed form immediately disables acceptance and retires the old review',
  );
  await session.start();
  heldView = new Promise((resolve) => {
    releaseView = resolve;
  });
  const poll = session.poll();
  snapshot = { ...snapshot, revision: snapshot.revision + 1 };
  await subtitles.command({ kind: 'status' });
  releaseView(sample);
  await poll;
  heldView = null;
  assert(
    !session.canAccept && !session.view && !session.formCurrent,
    'Late review revived a stale document',
  );
  passed('Late review cannot revive a proposal after document replacement');
  heldForm = new Promise((resolve) => {
    releaseForm = resolve;
  });
  const formRead = session.begin('edit', 'Title');
  snapshot = { ...snapshot, revision: snapshot.revision + 1 };
  await subtitles.command({ kind: 'status' });
  releaseForm(form);
  await formRead;
  heldForm = null;
  assert(session.mode === null, 'Late form overwrote a new document');
  passed(
    'A held form read validates the captured document before populating drafts',
  );
  await session.begin('edit', 'Title');
  session.fields.find((f) => f.name === 'Fontsize')!.value = '42';
  await session.start();
  const accepted = session.accept();
  assert(
    !session.canClose && !session.canStart,
    'Pending acceptance permits new work or close',
  );
  releaseAccept({ ...snapshot, revision: snapshot.revision + 1, dirty: true });
  assert(await accepted, 'Accepted snapshot not reconciled');
  assert(
    subtitles.snapshot.revision === snapshot.revision + 1,
    'Document owner did not adopt',
  );
  passed(
    'Acceptance protects dialog lifetime until the sole document owner reconciles',
  );
  session.dispose();
  const late = new StyleSession(subtitles, api);
  await late.begin('copy', 'Title');
  heldStart = new Promise((resolve) => {
    releaseStart = resolve;
  });
  const start = late.start();
  late.dispose();
  releaseStart(991);
  await start;
  assert(
    cancels.includes(991) && !late.view,
    'Disposed dialog did not cancel a late start',
  );
  passed(
    'Unmount cancels a late proposal identity without adopting its result',
  );
  subtitles.dispose();
}
