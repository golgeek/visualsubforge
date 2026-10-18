import { mount, unmount, tick } from 'svelte';
import App from '../../../src/App.svelte';
import {
  emptyDocument,
  emptyStatus,
  defaultPreferences,
  type DocumentSnapshot,
  type DocumentAction,
  type SceneRepairView,
} from '../../../src/lib/client';
import { sceneRepairView as sample } from '../ipc-contract-samples';
import { lifecycle } from './lifecycle';
const host = document.querySelector('#test')!;
const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
let doc: DocumentSnapshot = {
  ...emptyDocument,
  loaded: true,
  generation: sample.key.generation,
  revision: sample.key.revision,
  path: '/fixture.srt',
  cues: [
    { id: 1, startMs: 1000, endMs: 3000, text: '{\\i1}First é 😀' },
    { id: 2, startMs: 4050, endMs: 6050, text: 'Second' },
    { id: 3, startMs: 8000, endMs: 10000, text: '{\\k20}Timed' },
  ],
};
const original = structuredClone(doc);
let view: SceneRepairView = structuredClone(sample);
let before = structuredClone(doc);
let accepts = 0,
  starts = 0,
  edits = 0;
let failApply = false,
  failAccept = false;
let heldStart: Promise<number> | null = null;
const cancelled: number[] = [];
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  output.append(row);
}
function button(text: string) {
  const b = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!b) throw new Error(`Missing ${text}`);
  return b;
}
async function until(check: () => boolean) {
  await tick();
  const end = performance.now() + 6000;
  while (!check()) {
    assert(performance.now() < end, 'Timed out: ' + host.textContent);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
function type(text: string) {
  const input = host.querySelector<HTMLTextAreaElement>(
    '[aria-label="Subtitle text"]',
  )!;
  input.value = text;
  input.dispatchEvent(new Event('input', { bubbles: true }));
}
async function open() {
  await until(() => !button('Repair scene timing…').disabled);
  button('Repair scene timing…').click();
}
Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
  configurable: true,
  value: { unregisterListener: () => {} },
});
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    transformCallback: () => 1,
    unregisterCallback: () => {},
    invoke: async (
      command: string,
      args: {
        action?: DocumentAction;
        id?: number;
        offset?: number;
        limit?: number;
        acknowledgeNoMedia?: boolean;
      },
    ) => {
      switch (command) {
        case 'plugin:event|listen':
          return 1;
        case 'plugin:event|unlisten':
        case 'update_menu':
          return;
        case 'preferences_get':
          return structuredClone(defaultPreferences);
        case 'recovery_status':
          return { entries: [], error: null };
        case 'media_action':
          return structuredClone(emptyStatus);
        case 'scene_repair_start':
          starts++;
          view = {
            ...view,
            id: starts,
            key: {
              ...view.key,
              generation: doc.generation,
              revision: doc.revision,
            },
          };
          return heldStart ?? starts;
        case 'scene_repair_view':
          return {
            ...structuredClone(view),
            offset: args.offset ?? 0,
            rows: view.rows.slice(
              args.offset ?? 0,
              (args.offset ?? 0) + (args.limit ?? 100),
            ),
          };
        case 'scene_repair_cancel':
          cancelled.push(args.id!);
          return;
        case 'scene_repair_accept': {
          if (failAccept) throw new Error('Injected stale scene proposal');
          accepts++;
          before = structuredClone(doc);
          doc = {
            ...doc,
            revision: doc.revision + 1,
            dirty: true,
            canUndo: true,
            cues: doc.cues.map((c) => ({
              ...c,
              startMs:
                view.rows.find((r) => r.cueId === String(c.id))?.newStartMs ??
                c.startMs,
              endMs:
                view.rows.find((r) => r.cueId === String(c.id))?.newEndMs ??
                c.endMs,
            })),
          };
          return structuredClone(doc);
        }
        case 'quality_read':
          throw new Error('Quality must remain disabled in this fixture');
        case 'document_action': {
          const action = args.action!;
          if (action.kind === 'edit') {
            if (failApply) throw new Error('Injected draft failure');
            edits++;
            doc = {
              ...doc,
              revision: doc.revision + 1,
              dirty: true,
              cues: doc.cues.map((c) =>
                c.id === action.cueId ? { ...c, text: action.text } : c,
              ),
            };
          } else if (action.kind === 'undo') {
            doc = { ...before, revision: doc.revision + 1, canRedo: true };
          } else
            assert(action.kind === 'status', 'Unexpected document operation');
          return structuredClone(doc);
        }
        default:
          throw new Error(`Unexpected IPC ${command}`);
      }
    },
  },
});
const app = mount(App, { target: host });
async function run() {
  try {
    await until(() => host.querySelectorAll('.cue-row').length === 3);
    type('Keep this draft');
    await tick();
    await open();
    await until(
      () =>
        !!host.querySelector('dialog[open] [aria-label="Unapplied cue draft"]'),
    );
    assert(
      button('Create scene preview').disabled && starts === 0,
      'Draft bypassed preview protection',
    );
    button('Cancel scene review').click();
    await until(
      () =>
        !host.querySelector(
          'dialog[open][aria-labelledby="scene-repair-title"]',
        ),
    );
    assert(
      host.querySelector<HTMLTextAreaElement>('[aria-label="Subtitle text"]')!
        .value === 'Keep this draft' && edits === 0,
      'Cancel lost draft',
    );
    passed(
      'Opening or cancelling scene review preserves an unapplied draft and accepted state',
    );
    await open();
    await until(
      () =>
        !!host.querySelector(
          'dialog[open][aria-labelledby="scene-repair-title"]',
        ),
    );
    failApply = true;
    button('Apply cue draft').click();
    await until(() => host.textContent!.includes('Injected draft failure'));
    assert(starts === 0 && edits === 0, 'Failed Apply started proposal');
    button('Discard cue draft').click();
    await tick();
    failApply = false;
    button('Create scene preview').click();
    await until(() => host.textContent!.includes('2 safe changes'));
    assert(
      JSON.stringify(doc) === JSON.stringify(original),
      'Preview changed document',
    );
    assert(
      !button('Accept scene changes').disabled,
      'Completed scene plan not acceptable',
    );
    passed(
      'Failed draft Apply preserves state; Discard permits a read-only completed-scene preview',
    );
    for (const coverage of ['noMedia', 'unavailable', 'partial'] as const) {
      view = { ...view, coverage };
      await until(() => button('Accept scene changes').disabled);
      assert(
        !host.querySelector('dialog input[type="checkbox"]'),
        'Missing scenes could be acknowledged away',
      );
      view = { ...view, coverage: 'checked' };
      await until(() => !button('Accept scene changes').disabled);
    }
    view = { ...view, coverage: 'reduced' };
    await until(() =>
      host.textContent!.includes('Reduced detection can miss cuts'),
    );
    assert(
      !button('Accept scene changes').disabled,
      'Completed Reduced scan unnecessarily disabled',
    );
    passed(
      'Missing and partial scenes prevent acceptance; Reduced detection carries an explicit heuristic limitation',
    );
    failAccept = true;
    button('Accept scene changes').click();
    await until(() =>
      host.textContent!.includes('Injected stale scene proposal'),
    );
    assert(
      accepts === 0 && JSON.stringify(doc) === JSON.stringify(original),
      'Failed acceptance mutated document',
    );
    failAccept = false;
    button('Accept scene changes').click();
    await until(
      () =>
        accepts === 1 &&
        !host.querySelector(
          'dialog[open][aria-labelledby="scene-repair-title"]',
        ),
    );
    assert(
      doc.revision === original.revision + 1 &&
        doc.cues[0]!.startMs === 1130 &&
        doc.cues[1]!.endMs === 5870,
      'Accepted timing mismatch',
    );
    document
      .querySelector<HTMLTextAreaElement>('[aria-label="Subtitle text"]')!
      .focus();
    button('Undo edit').click();
    await until(() => doc.cues[0]!.startMs === 1000);
    assert(
      JSON.stringify(doc.cues) === JSON.stringify(original.cues),
      'Undo did not restore all cues',
    );
    passed(
      'Failed acceptance preserves state; explicit acceptance reconciles one revision and one Undo restores all timings',
    );
    view = {
      ...structuredClone(sample),
      total: 205,
      changed: 0,
      skipped: 205,
      rows: Array.from({ length: 205 }, (_, i) => ({
        ...sample.rows[0]!,
        cueId: String(i + 1),
        cueIndex: i,
        newStartMs: null,
        newEndMs: null,
        skipped: 'durationConstraint',
        explanation: 'Would violate minimum duration',
      })),
    };
    await open();
    await until(
      () =>
        !!host.querySelector(
          'dialog[open][aria-labelledby="scene-repair-title"]',
        ),
    );
    button('Create scene preview').click();
    await until(() => host.querySelectorAll('dialog tbody tr').length === 100);
    assert(
      button('Accept scene changes').disabled,
      'All-skipped proposal was acceptable',
    );
    button('Next candidates').click();
    await until(() => host.textContent!.includes('101–200 of 205'));
    assert(
      host.querySelectorAll('dialog tbody tr').length === 100,
      'Unbounded proposal rows',
    );
    passed(
      'Skipped reasons and 100-row proposal paging stay reviewable without enabling an empty acceptance',
    );
    view = { ...view, phase: 'stale' };
    await until(() => host.textContent!.includes('quality profile changed'));
    assert(
      button('Accept scene changes').disabled,
      'Stale proposal remains acceptable',
    );
    button('Close scene review').click();
    await until(
      () =>
        !host.querySelector(
          'dialog[open][aria-labelledby="scene-repair-title"]',
        ),
    );
    passed(
      'Background stale context detection retires an already displayed preview',
    );
    let release!: (id: number) => void;
    heldStart = new Promise((resolve) => {
      release = resolve;
    });
    await open();
    await until(
      () =>
        !!host.querySelector(
          'dialog[open][aria-labelledby="scene-repair-title"]',
        ),
    );
    const prior = starts;
    button('Create scene preview').click();
    await until(() => starts > prior);
    button('Close scene review').click();
    await until(
      () =>
        !host.querySelector(
          'dialog[open][aria-labelledby="scene-repair-title"]',
        ),
    );
    release(999);
    heldStart = null;
    await until(() => cancelled.includes(999));
    passed(
      'Closing during admission cancels the late returned proposal identity',
    );
    await unmount(app);
    await lifecycle(sample, original, passed);
    heading.textContent = `PASS: all ${output.children.length} scene review checks`;
  } catch (e) {
    heading.textContent = `FAIL: ${String(e)}`;
    await unmount(app);
  }
}
void run();
