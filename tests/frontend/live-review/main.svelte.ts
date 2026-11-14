import { tick } from 'svelte';
import { GapSession } from '../../../src/lib/timing/gap-session.svelte';
import { DocumentSession } from '../../../src/lib/document/document-session.svelte';
import {
  emptyDocument,
  type GapView,
  type ReviewSelection,
} from '../../../src/lib/client';
import { gapView as sample } from '../ipc-contract-samples';
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const li = document.createElement('li');
  li.textContent = `PASS: ${message}`;
  output.append(li);
}
async function until(check: () => boolean) {
  const end = performance.now() + 6000;
  while (!check()) {
    assert(performance.now() < end, 'Timed out waiting for live review');
    await new Promise((resolve) => setTimeout(resolve, 20));
    await tick();
  }
}
const snapshot = {
  ...emptyDocument,
  loaded: true,
  generation: sample.key.generation,
  revision: sample.key.revision,
  cues: [
    { id: 1, startMs: 1000, endMs: 2000, text: 'First' },
    { id: 2, startMs: 2050, endMs: 3050, text: 'Second' },
  ],
};
let mutations = 0;
const subtitles = new DocumentSession({
  chooseFile: async () => null,
  documentAction: async () => structuredClone(snapshot),
  gapAccept: async () => {
    mutations++;
    return snapshot;
  },
  resyncAccept: async () => {
    throw new Error('Unused');
  },
});
let selection = $state<ReviewSelection>({ scope: null, excluded: [] });
let context = $state('initial');
let enabled = $state(true);
let heartbeat = $state(0);
const heartbeatTimer = setInterval(() => heartbeat++, 50);
let ids = $state([1, 2]);
let starts = 0;
let failures = 0;
let held: Promise<number> | null = null;
const cancelled: number[] = [];
// eslint-disable-next-line svelte/prefer-svelte-reactivity -- Fake server storage is never observed by the component.
const plans = new Map<
  number,
  { view: GapView; subset: ReviewSelection | null }
>();
let session: GapSession;
let destroy = () => {};
async function run() {
  try {
    await subtitles.command({ kind: 'status' });
    destroy = $effect.root(() => {
      session = new GapSession(
        subtitles,
        () => 0,
        {
          gapStart: async (_media, doc, subset = null) => {
            const id = ++starts;
            if (failures) {
              failures--;
              throw new Error('Injected admission failure');
            }
            const view: GapView = structuredClone(sample);
            view.id = id;
            view.key.generation = doc.generation;
            view.key.revision = doc.revision;
            view.rows = view.rows.map((row) => ({
              ...row,
              selected: !subset?.excluded.includes(Number(row.cueId)),
            }));
            view.changed = view.rows.filter((row) => row.selected).length;
            plans.set(id, { view, subset });
            return held ?? id;
          },
          gapView: async (id, offset = 0, limit = 100, projection) => {
            assert(limit <= 200, 'Unbounded preview read');
            const plan = plans.get(id)!;
            return {
              ...structuredClone(plan.view),
              offset,
              rows: plan.view.rows
                .filter(
                  (row) =>
                    !projection || projection.includes(Number(row.cueId)),
                )
                .slice(offset, offset + limit),
            };
          },
          gapCancel: async (id) => {
            cancelled.push(id);
            const plan = plans.get(id);
            if (plan && plan.view.phase !== 'stale')
              plan.view.phase = 'cancelled';
          },
        },
        {
          enabled: () => {
            void heartbeat;
            return enabled;
          },
          selection: () => selection,
          context: () => context,
          previewIds: () => ids,
        },
      );
      return () => session.dispose();
    });
    await until(() => !!session.preview);
    assert(
      starts === 1 && session.canAccept && mutations === 0,
      'Automatic read-only preview admission failed',
    );
    passed(
      'Live admission produces a bounded read-only preview without mutating accepted subtitles',
    );
    selection = { scope: null, excluded: [1] };
    await tick();
    assert(!session.canAccept, 'Input change retained acceptance');
    await until(() => session.preview?.changed === 1);
    assert(
      cancelled.includes(1) &&
        !session.preview?.rows.find((row) => row.cueId === '1')?.selected,
      'Unchecked change survived replacement',
    );
    passed(
      'Unticking immediately invalidates acceptance, cancels the old proposal and projects the Rust-selected subset',
    );
    const prior = starts;
    ids = [2];
    await until(() => session.preview?.rows.length === 1);
    assert(
      starts === prior && session.preview?.rows[0]?.cueId === '2',
      'Viewport change rebuilt the proposal',
    );
    passed(
      'Viewport changes refresh only bounded ghost rows without restarting calculation',
    );
    const beforeRefresh = starts;
    await subtitles.command({ kind: 'status' });
    assert(
      !session.current,
      'Replacement snapshot retained a captured proposal',
    );
    await until(() => starts > beforeRefresh && !!session.preview);
    passed(
      'An authoritative replacement snapshot restarts review even when generation and revision are unchanged',
    );
    const staleId = starts;
    plans.get(staleId)!.view.phase = 'stale';
    await until(() => session.view?.phase === 'stale');
    assert(!session.canAccept, 'Backend-stale plan remained acceptable');
    session.retry();
    await until(() => starts > staleId && !!session.preview);
    assert(cancelled.includes(staleId), 'Retry failed to retire the stale job');
    passed(
      'Explicit retry replaces a backend-stale job even when cancellation retains its stale terminal phase',
    );
    enabled = false;
    await until(() => session.view?.phase === 'cancelled');
    assert(
      !session.preview && !session.canAccept,
      'Disabled review retained ghosts or acceptance',
    );
    failures = 5;
    enabled = true;
    context = 'failure';
    const before = starts;
    await until(() => starts === before + 3);
    await new Promise((resolve) => setTimeout(resolve, 700));
    assert(
      starts === before + 3 && session.error.includes('Injected'),
      'Read-only retry was unbounded',
    );
    failures = 0;
    session.retry();
    await until(() => !!session.preview);
    passed(
      'Admission failures stop after three read-only attempts and recover only after explicit retry or changed input',
    );
    let release!: (id: number) => void;
    held = new Promise((resolve) => (release = resolve));
    const priorHeld = starts;
    context = 'held';
    await until(() => starts > priorHeld);
    const admitted = starts;
    destroy();
    release(admitted);
    await until(() => cancelled.includes(admitted));
    assert(
      !session.preview && mutations === 0,
      'Disposal retained a late preview',
    );
    passed(
      'Disposal cancels a late admission and never replays an accepted mutation',
    );
    clearInterval(heartbeatTimer);
    heading.textContent = `PASS: all ${output.children.length} live review checks`;
  } catch (error) {
    clearInterval(heartbeatTimer);
    heading.textContent = `FAIL: ${String(error)}`;
    destroy();
  }
}
void run();
