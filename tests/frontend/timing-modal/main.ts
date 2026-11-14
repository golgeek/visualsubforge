import { emptyQuality } from '../empty-quality';
import { chooseCommand } from '../command-discovery';
import { mount, unmount, tick } from 'svelte';
import App from '../../../src/App.svelte';
import {
  emptyDocument,
  emptyStatus,
  defaultPreferences,
  type DocumentAction,
  type MediaAction,
  type WorkspaceAction,
} from '../../../src/lib/client';
import {
  gapView,
  sceneRepairView,
  durationRepairView,
  batchTimingView,
} from '../ipc-contract-samples';

const host = document.querySelector('#test')!;
const output = document.querySelector('#checks')!;
const heading = document.querySelector('#result')!;
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  output.append(row);
}
function deferred() {
  let resolve!: () => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<void>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
async function settle() {
  await tick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  await tick();
}
async function until(check: () => boolean) {
  const end = performance.now() + 6000;
  while (!check()) {
    assert(performance.now() < end, `Timed out: ${host.textContent}`);
    await settle();
  }
  await settle();
}
function button(label: string) {
  const result = [
    ...host.querySelectorAll<HTMLButtonElement>(
      'button:not(dialog:not([open]) button)',
    ),
  ].find((b) => b.textContent?.trim() === label);
  assert(result, `Missing button: ${label}`);
  return result!;
}
const variants = [
  {
    name: 'gap',
    launcher: 'Fix gaps and overlaps…',
    close: 'Cancel',
    preview: 'Preview changes',
    accept: 'Apply changes',
    prefix: 'gap',
    sample: gapView,
  },
  {
    name: 'scene',
    launcher: 'Snap to shot changes…',
    close: 'Cancel',
    preview: 'Preview changes',
    accept: 'Apply changes',
    prefix: 'scene_repair',
    sample: sceneRepairView,
  },
  {
    name: 'duration',
    launcher: 'Fix reading speed…',
    close: 'Cancel',
    preview: 'Preview changes',
    accept: 'Apply changes',
    prefix: 'duration_repair',
    sample: durationRepairView,
  },
  {
    name: 'batch',
    launcher: 'Shift / align cues…',
    close: 'Cancel',
    preview: 'Preview changes',
    accept: 'Apply changes',
    prefix: 'batch_timing',
    sample: batchTimingView,
  },
] as const;
async function fixture(variant: (typeof variants)[number], above = true) {
  // Discovery has its own lifecycle harness. Delay/count the feature's native
  // hide after the Commands menu closes, preserving the feature boundary tests.
  let discovering = false;
  const calls: string[] = [];
  const cancels: number[] = [];
  let occluded = false;
  let hide: ReturnType<typeof deferred> | null = null;
  let restore: ReturnType<typeof deferred> | null = null;
  let mutation: ReturnType<typeof deferred> | null = null;
  let start: ReturnType<typeof deferred> | null = null;
  let handler = 0;
  const callbacks = new Map<number, (event: unknown) => void>();
  let doc = {
    ...emptyDocument,
    loaded: true,
    generation: variant.sample.key.generation,
    revision: variant.sample.key.revision,
    path: '/timing.srt',
    cues: [
      { id: 1, startMs: 1000, endMs: 2000, text: 'First' },
      { id: 2, startMs: 2050, endMs: 3050, text: 'Second' },
    ],
  };
  Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
  Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
    configurable: true,
    value: { unregisterListener: () => {} },
  });
  Object.defineProperty(window, '__TAURI_INTERNALS__', {
    configurable: true,
    value: {
      transformCallback: (callback: (event: unknown) => void) => {
        const id = callbacks.size + 1;
        callbacks.set(id, callback);
        return id;
      },
      unregisterCallback: () => {},
      invoke: async (
        command: string,
        args: {
          action?: DocumentAction | MediaAction;
          handler?: number;
          event?: string;
          id?: number;
        },
      ) => {
        switch (command) {
          case 'quality_read':
            return emptyQuality(args);
          case 'quality_cancel':
            return null;
          case 'plugin:event|listen':
            if (args.event !== 'workspace-action') return 2;
            handler = args.handler!;
            return 1;
          case 'plugin:event|unlisten':
          case 'update_menu':
            return;
          case 'preferences_get':
            return structuredClone(defaultPreferences);
          case 'recovery_status':
            return { entries: [], error: null };
          case 'media_action': {
            const action = args.action as MediaAction;
            if (action.kind === 'occlude') {
              if (discovering) {
                if (!action.occluded) discovering = false;
              } else {
                calls.push(`occlude:${action.occluded}`);
                await (action.occluded ? hide?.promise : restore?.promise);
              }
              occluded = action.occluded;
            } else if (action.kind !== 'status' && action.kind !== 'layout')
              calls.push(`media:${action.kind}`);
            return {
              ...emptyStatus,
              available: true,
              surfaceAboveWebview: above,
            };
          }
          case 'document_action': {
            const action = args.action as DocumentAction;
            if (action.kind !== 'status') {
              calls.push(`document:${action.kind}`);
              await mutation?.promise;
              if (action.kind === 'edit')
                doc = {
                  ...doc,
                  revision: doc.revision + 1,
                  cues: doc.cues.map((c) =>
                    c.id === action.cueId ? { ...c, text: action.text } : c,
                  ),
                };
            }
            return structuredClone(doc);
          }
        }
        if (command === `${variant.prefix}_start`) {
          calls.push('start');
          await start?.promise;
          return 999;
        }
        if (command === `${variant.prefix}_view`)
          return {
            ...structuredClone(variant.sample),
            id: 999,
            key: {
              ...variant.sample.key,
              generation: doc.generation,
              revision: doc.revision,
            },
            coverage: variant.name === 'batch' ? undefined : 'checked',
          };
        if (command === `${variant.prefix}_cancel`) {
          cancels.push(args.id!);
          return;
        }
        if (command === `${variant.prefix}_accept`) {
          calls.push('accept');
          await mutation?.promise;
          doc = { ...doc, revision: doc.revision + 1 };
          return structuredClone(doc);
        }
        calls.push(command);
        throw new Error(`Unexpected IPC: ${command}`);
      },
    },
  });
  const app = mount(App, { target: host });
  await until(
    () =>
      !!callbacks.get(handler) &&
      [
        ...host.querySelectorAll<HTMLButtonElement>(
          'button:not(dialog:not([open]) button)',
        ),
      ].some((b) => b.textContent?.trim() === 'Commands' && !b.disabled),
  );
  // Media polling has started synchronously on mount; allow its projection to render.
  await settle();
  const launcher = button('Commands');
  const begin = () => {
    discovering = true;
    void chooseCommand(
      host,
      (
        {
          gap: 'fixGaps',
          scene: 'snapShots',
          duration: 'fixReadingSpeed',
          batch: 'shiftCues',
        } as const
      )[variant.name],
    );
  };
  const dialog = () =>
    host.querySelector<HTMLDialogElement>('[aria-label="Review panel"]');
  const action = (value: WorkspaceAction) =>
    callbacks.get(handler)!({ payload: value });
  return {
    calls,
    begin,
    cancels,
    launcher,
    dialog,
    action,
    occluded: () => occluded,
    holdHide: () => (hide = deferred()),
    holdRestore: () => (restore = deferred()),
    holdMutation: () => (mutation = deferred()),
    holdStart: () => (start = deferred()),
    open: async () => {
      launcher.focus();
      begin();
      await until(() => !!dialog());
    },
    close: async () => {
      button('Close').click();
      await until(() => !dialog() && !launcher.disabled);
    },
    dispose: () => unmount(app),
  };
}
async function blocked(f: Awaited<ReturnType<typeof fixture>>) {
  const before = f.calls.length;
  for (const action of [
    'settings',
    'plugins',
    'openMedia',
    'closeMedia',
    'openProject',
    'quit',
  ] as const)
    f.action(action);
  window.dispatchEvent(
    new KeyboardEvent('keydown', {
      key: 'f',
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    }),
  );
  button('Commands').click();
  await settle();
  assert(
    f.calls.length === before && f.launcher.disabled,
    'Transition admitted another workspace action',
  );
}
async function run() {
  for (const variant of variants) {
    for (const dispose of [false, true]) {
      const f = await fixture(variant);
      let disposed = false;
      try {
        await f.open();
        await until(
          () =>
            !!host.querySelector<HTMLButtonElement>('.timing-review .apply') &&
            !host.querySelector<HTMLButtonElement>('.timing-review .apply')!
              .disabled,
        );
        assert(
          !f.occluded() && !f.calls.some((call) => call.startsWith('occlude:')),
          `${variant.name}: dock added an occlusion handshake`,
        );
        const dock = f.dialog()!.getBoundingClientRect();
        const video = host
          .querySelector('[aria-label="Native video surface"]')!
          .getBoundingClientRect();
        assert(
          dock.top >= video.bottom ||
            dock.right <= video.left ||
            dock.left >= video.right,
          'Dock overlaps native video',
        );
        const reply = f.holdMutation();
        host.querySelector<HTMLButtonElement>('.timing-review .apply')!.click();
        await until(() => f.calls.includes('accept'));
        assert(
          button('Close').disabled,
          'Acceptance allowed close before acknowledgment',
        );
        assert(
          [
            ...host.querySelectorAll<HTMLButtonElement>(
              '[aria-label="Review tools"] button',
            ),
          ].every((button) => button.disabled),
          'Acceptance allowed tab disposal',
        );
        await blocked(f);
        if (dispose) {
          await f.dispose();
          disposed = true;
        }
        const count = f.calls.length;
        reply.resolve();
        await settle();
        if (dispose)
          assert(
            f.calls.length === count &&
              !host.textContent?.includes('timings accepted'),
            'Disposed acceptance sent follow-up or notice',
          );
        else {
          await until(() => !button('Close').disabled);
          await f.close();
          assert(
            document.activeElement === f.launcher,
            'Dock close lost launcher focus',
          );
        }
      } finally {
        if (!disposed) await f.dispose();
      }
    }
    passed(
      `${variant.name}: dock never overlaps or hides video; acceptance blocks workspace actions and tab/close disposal, with late presentation suppressed after teardown`,
    );
    const f = await fixture(variant);
    try {
      const late = f.holdStart();
      await f.open();
      await until(() => f.calls.includes('start'));
      await f.close();
      late.resolve();
      await until(() => f.cancels.includes(999));
      assert(!f.dialog(), 'Late admission revived dock');
    } finally {
      await f.dispose();
    }
    passed(
      `${variant.name}: closing a read-only admission cancels the eventual job and restores workspace focus`,
    );
  }
  {
    const f = await fixture(variants[0]);
    try {
      f.action('diagnostics');
      await until(() => f.occluded());
      await f.open();
      await f.close();
      assert(f.occluded(), 'Closing dock exposed video under diagnostics');
    } finally {
      await f.dispose();
    }
    passed(
      'Independent diagnostics retains HideVideo while the dock opens and closes',
    );
  }
  {
    const f = await fixture(variants[0], false);
    try {
      await f.open();
      await f.close();
      assert(
        !f.calls.some((call) => call.startsWith('occlude:')) &&
          document.activeElement === f.launcher,
        'Below-WebView path lost focus or sent occlusion',
      );
    } finally {
      await f.dispose();
    }
    passed(
      'Below-WebView capability requires no occlusion and retains focus restoration',
    );
  }
  {
    const f = await fixture(variants[0]);
    try {
      await f.open();
      f.launcher.remove();
      let focused = false;
      f.launcher.focus = () => {
        focused = true;
      };
      button('Close').click();
      await until(() => !f.dialog());
      assert(!focused, 'Close focused a detached launcher');
    } finally {
      await f.dispose();
    }
    passed('Closing a dock never focuses a detached launcher');
  }
  heading.textContent = `PASS: all ${output.children.length} timing dock checks`;
}
void run().catch((error) => {
  heading.textContent = `FAIL: ${String(error)}`;
});
