import { mount, tick, unmount } from 'svelte';
import App from '../../../src/App.svelte';
import {
  defaultPreferences,
  emptyDocument,
  emptyStatus,
  type DocumentAction,
  type DocumentSnapshot,
  type MediaAction,
  type QualityRequest,
  type QualityView,
  type updateMenu,
} from '../../../src/lib/client';
import { emptyQuality } from '../empty-quality';
const host = document.querySelector('#test')!;
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
  const deadline = performance.now() + 6000;
  while (!check()) {
    if (performance.now() > deadline)
      throw new Error(
        `UI response timeout (${stage}): selected ${selectedForChecks}; editor ${editor()?.value}; chips ${host.querySelectorAll('.issue-strip [aria-pressed]').length}; busy ${menuState?.busy}; text ${host.querySelector('.issue-strip')?.textContent}`,
      );
    await new Promise((r) => setTimeout(r, 15));
  }
  await tick();
}
function button(label: string) {
  const found = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!found) throw new Error(`Missing ${label}`);
  return found;
}
function editor() {
  return host.querySelector<HTMLTextAreaElement>(
    '[aria-label="Subtitle text"]',
  )!;
}
function type(text: string) {
  editor().focus();
  editor().value = text;
  editor().dispatchEvent(new Event('input', { bubbles: true }));
}
function escape() {
  editor().dispatchEvent(
    new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
  );
}
const original: DocumentSnapshot = {
  ...emptyDocument,
  loaded: true,
  generation: 1,
  revision: 0,
  cues: [
    { id: 1, startMs: 1000, endMs: 4000, text: 'One issue' },
    { id: 2, startMs: 5000, endMs: 8000, text: 'Three issues' },
    { id: 3, startMs: 9000, endMs: 12000, text: 'Five issues' },
  ],
};
let snapshot = structuredClone(original);
let stage = 'start';
const history: DocumentSnapshot[] = [];
const commands: string[] = [];
let editCount = 0;
let fixCount = 0;
let rejectEdit = false;
let fixId = 0;
const plans = new Map<number, { cue: number; version: number }>();
let selectedForChecks = 1;
let menuState: Parameters<typeof updateMenu>[0] | undefined;
const callbacks = new Map<number, (value: unknown) => void>();
let handler = 0;
const media = {
  ...emptyStatus,
  session: 1,
  available: true,
  surfaceAboveWebview: true,
  loaded: true,
  paused: true,
  durationSeconds: 20,
  positionSeconds: 2,
  nominalFrameRate: 25,
};
let occluded = false;
let releaseHide: (() => void) | null = null;
let holdHide = false;
function issueProjection(
  cueId: number,
): NonNullable<QualityView['selectedIssues']> {
  const rules = [
    'rs_high',
    'line_length',
    'duration_short',
    'gap',
    'scene_end',
  ] as const;
  const count = cueId === 1 ? 1 : cueId === 2 ? 3 : 5;
  return {
    cueId: String(cueId),
    count,
    offset: 0,
    issues: rules.slice(0, count).map((rule) => ({
      rule,
      findings: [
        {
          rule,
          cueId: String(cueId),
          cueIndex: cueId - 1,
          relatedCueId: null,
          severity: 'violation',
          measured: 50,
          expected: 'Review the active profile',
          explanation: `${rule} detail for cue ${cueId}`,
          language: null,
        },
      ],
    })),
  };
}
Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
  configurable: true,
  value: { unregisterListener() {} },
});
Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    transformCallback(callback: (value: unknown) => void) {
      const id = callbacks.size + 1;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback() {},
    async invoke(
      command: string,
      args: {
        action?: DocumentAction | MediaAction;
        request?: QualityRequest;
        state?: Parameters<typeof updateMenu>[0];
        handler?: number;
        cueId?: number;
        id?: number;
        revision?: number;
      },
    ) {
      switch (command) {
        case 'preferences_get':
          return structuredClone(defaultPreferences);
        case 'recovery_status':
          return { entries: [], error: null };
        case 'plugin:event|listen':
          handler = args.handler!;
          return 1;
        case 'plugin:event|unlisten':
          return null;
        case 'update_menu':
          menuState = args.state;
          return null;
        case 'quality_cancel':
          return null;
        case 'quality_read': {
          const view = emptyQuality(args);
          const request = args.request!;
          if (request.selected !== null) selectedForChecks = request.selected;
          const projection =
            request.selected === null
              ? null
              : issueProjection(request.selected);
          return {
            ...view,
            selectedIssues: projection,
            findingsCount: 0,
            findings: [],
          };
        }
        case 'cue_fixes_start': {
          const id = ++fixId;
          plans.set(id, { cue: args.cueId!, version: args.revision! });
          return id;
        }
        case 'cue_fixes_view': {
          const plan = plans.get(args.id!)!;
          return {
            id: args.id,
            key: {
              generation: 1,
              revision: plan.version,
              epoch: 0,
              loaded: true,
            },
            cueId: String(plan.cue),
            phase: 'reviewing',
            error: null,
            fixed: plan.cue === 3 ? 2 : 1,
            remaining: 3,
          };
        }
        case 'cue_fixes_cancel':
          return null;
        case 'cue_fixes_accept': {
          const plan = plans.get(args.id!)!;
          assert(plan.version === snapshot.revision, 'Stale fix accepted');
          history.push(structuredClone(snapshot));
          fixCount++;
          commands.push('fix');
          snapshot = {
            ...snapshot,
            revision: snapshot.revision + 1,
            canUndo: true,
            dirty: true,
            cues: snapshot.cues.map((c) =>
              c.id === plan.cue
                ? { ...c, text: 'Fixed\ntext', endMs: c.endMs + 100 }
                : c,
            ),
          };
          return structuredClone(snapshot);
        }
        case 'media_action': {
          const action = args.action as MediaAction;
          if (action.kind === 'occlude') {
            if (holdHide && action.occluded)
              await new Promise<void>((resolve) => (releaseHide = resolve));
            occluded = action.occluded;
          }
          if (action.kind === 'seek') media.positionSeconds = action.seconds;
          return structuredClone(media);
        }
        case 'document_action': {
          const action = args.action as DocumentAction;
          if (action.kind === 'status') return structuredClone(snapshot);
          commands.push(action.kind);
          if (action.kind === 'edit') {
            editCount++;
            if (rejectEdit)
              throw { kind: 'rejected', message: 'Modeled invalid timing' };
            assert(
              args.revision === snapshot.revision,
              'Edit used stale version',
            );
            history.push(structuredClone(snapshot));
            snapshot = {
              ...snapshot,
              revision: snapshot.revision + 1,
              canUndo: true,
              dirty: true,
              cues: snapshot.cues.map((c) =>
                c.id === action.cueId
                  ? {
                      ...c,
                      text: action.text,
                      startMs: action.startMs,
                      endMs: action.endMs,
                    }
                  : c,
              ),
            };
          } else if (action.kind === 'undo') {
            const previous = history.pop()!;
            snapshot = {
              ...previous,
              revision: snapshot.revision + 1,
              canRedo: true,
            };
          } else if (action.kind === 'split') {
            const cue = snapshot.cues.find((c) => c.id === action.cueId)!;
            history.push(structuredClone(snapshot));
            snapshot = {
              ...snapshot,
              revision: snapshot.revision + 1,
              canUndo: true,
              cues: snapshot.cues.flatMap((c) =>
                c === cue
                  ? [
                      { ...c, endMs: action.atMs },
                      { ...c, id: 4, startMs: action.atMs },
                    ]
                  : [c],
              ),
            };
          } else throw new Error(`Unexpected document action ${action.kind}`);
          return structuredClone(snapshot);
        }
        default:
          throw new Error(`Unexpected IPC ${command}`);
      }
    },
  },
});
async function select(id: number) {
  await until(() => !!menuState && !menuState.busy);
  host.querySelector<HTMLButtonElement>(`[data-cue-id="${id}"]`)!.click();
  await until(
    () =>
      selectedForChecks === id &&
      !menuState?.busy &&
      editor().value === snapshot.cues.find((c) => c.id === id)!.text,
  );
}
async function run() {
  const app = mount(App, { target: host });
  try {
    await until(() => !!menuState && !menuState.busy && !!editor());
    assert(
      ![...host.querySelectorAll('.editor button')].some((b) =>
        ['Apply', 'Revert'].includes(b.textContent!.trim()),
      ),
      'Explicit draft controls remain',
    );
    type('Pause commit');
    await until(() => snapshot.cues[0]!.text === 'Pause commit');
    assert(
      editCount === 1 && !editor().readOnly,
      'Pause did not commit exactly once',
    );
    type('Blur commit');
    editor().blur();
    await until(() => snapshot.cues[0]!.text === 'Blur commit');
    assert(editCount === 2, 'Blur duplicated commit');
    type('Discard unsent');
    escape();
    await until(
      () =>
        editor().value === 'Blur commit' &&
        document.activeElement?.tagName === 'CANVAS',
    );
    assert(editCount === 2, 'Escape sent an unsent draft');
    passed(
      'Editor has no Apply/Revert; pause and blur commit, Escape discards unsent text',
    );

    type('Before selection');
    await select(2);
    assert(
      snapshot.cues[0]!.text === 'Before selection' && editCount === 3,
      'Selection lost latest typing',
    );
    passed('Moving to another cue commits the previous text first');
    const heights: number[] = [];
    for (const id of [1, 2, 3]) {
      stage = `select ${id}`;
      await select(id);
      stage = `chips ${id}`;
      await until(
        () =>
          host.querySelectorAll('.issue-strip [aria-pressed]').length ===
          Math.min(id === 1 ? 1 : id === 2 ? 3 : 5, 3),
      );
      heights.push(
        host.querySelector('.issue-strip')!.getBoundingClientRect().height,
      );
    }
    assert(
      heights.every((height) => height === heights[0]),
      'Issue count changed strip height',
    );
    stage = 'hold hide';
    holdHide = true;
    button('+2').click();
    await until(() => releaseHide !== null);
    assert(
      !host.querySelector('[aria-label="All cue issues"]'),
      'Issues covered video before hide acknowledgment',
    );
    stage = 'show overflow';
    holdHide = false;
    releaseHide!();
    releaseHide = null;
    await until(() => !!host.querySelector('[aria-label="All cue issues"]'));
    assert(
      occluded &&
        host.querySelectorAll('[aria-label="All cue issues"] li').length === 5,
      'Overflow omitted issues or native hide',
    );
    button('Close').click();
    await until(() => !host.querySelector('[aria-label="All cue issues"]'));
    passed(
      'One, three and five issues keep fixed height; overflow waits for HideVideo',
    );

    await until(() =>
      [...host.querySelectorAll('button')].some(
        (b) => b.textContent?.trim() === 'Fix 2',
      ),
    );
    const before = structuredClone(snapshot);
    button('Fix 2').click();
    await until(() => fixCount === 1 && editor().value === 'Fixed\ntext');
    assert(
      snapshot.revision === before.revision + 1,
      'Fix dispatched multiple accepted edits',
    );
    button('Undo edit').click();
    await until(() => editor().value === before.cues[2]!.text);
    assert(
      JSON.stringify(snapshot.cues) === JSON.stringify(before.cues),
      'One Undo failed to restore full modeled fix',
    );
    passed(
      'Fix N sends one acceptance and one Undo restores text and timing (modeled IPC)',
    );

    const oldStart = snapshot.cues[2]!.startMs;
    host
      .querySelector<HTMLButtonElement>(
        '[aria-label="Start forward one frame"]',
      )!
      .click();
    await until(() => snapshot.cues[2]!.startMs === oldStart + 40);
    passed(
      'Frame nudge uses supplied cadence while exact fields remain editable',
    );
    rejectEdit = true;
    type('Retain rejected edit');
    await until(() => !!host.querySelector('#cue-commit-error'));
    assert(
      editor().value === 'Retain rejected edit' &&
        editor().getAttribute('aria-invalid') === 'true',
      'Rejection erased text or omitted error',
    );
    const rejectedCount = editCount;
    await new Promise((r) => setTimeout(r, 850));
    assert(editCount === rejectedCount, 'Rejected edit retried automatically');
    escape();
    await until(() => editor().value === snapshot.cues[2]!.text);
    rejectEdit = false;
    passed(
      'Rejected commit remains inline without retry loop; Escape restores accepted text',
    );
    await select(1);
    media.positionSeconds = 2;
    await new Promise((r) => setTimeout(r, 250));
    type('Split mid-draft');
    const firstCommand = commands.length;
    callbacks.get(handler)!({ payload: 'splitCue' });
    await until(() => snapshot.cues.length === 4);
    assert(
      commands.slice(firstCommand).join(',') === 'edit,split',
      'Split did not commit text first',
    );
    assert(
      snapshot.cues.filter((c) => c.text === 'Split mid-draft').length === 2,
      'Split used old text',
    );
    passed(
      'Split remains available mid-draft and executes after the text commit',
    );
    heading.textContent = `PASS: all ${output.children.length} cue editor checks`;
  } finally {
    await unmount(app);
  }
}
void run().catch((error) => (heading.textContent = `FAIL: ${String(error)}`));
