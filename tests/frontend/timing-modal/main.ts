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
          id?: number;
        },
      ) => {
        switch (command) {
          case 'plugin:event|listen':
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
              calls.push(`occlude:${action.occluded}`);
              await (action.occluded ? hide?.promise : restore?.promise);
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
      ].some((b) => b.textContent?.trim() === variant.launcher && !b.disabled),
  );
  // Media polling has started synchronously on mount; allow its projection to render.
  await settle();
  const launcher = button(variant.launcher);
  const dialog = () => host.querySelector<HTMLDialogElement>('dialog[open]');
  const action = (value: WorkspaceAction) =>
    callbacks.get(handler)!({ payload: value });
  return {
    calls,
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
      launcher.click();
      await until(() => !!dialog());
    },
    close: async () => {
      button(variant.close).click();
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
  for (const v of variants) button(v.launcher).click();
  await settle();
  assert(
    f.calls.length === before && f.launcher.disabled,
    'Transition admitted another workspace action',
  );
}
async function run() {
  for (const variant of variants) {
    const f = await fixture(variant);
    try {
      const hide = f.holdHide();
      f.launcher.focus();
      f.launcher.click();
      await until(() => f.calls.includes('occlude:true'));
      assert(!f.dialog(), `${variant.name}: content mounted before hide`);
      await blocked(f);
      hide.resolve();
      await until(() => !!f.dialog());
      assert(f.occluded(), 'Visible dialog without occlusion');
      const restore = f.holdRestore();
      const closingDialog = f.dialog()!;
      button(variant.close).click();
      closingDialog.dispatchEvent(new Event('cancel', { cancelable: true }));
      await until(() => f.calls.includes('occlude:false'));
      assert(!f.dialog(), 'Restore before content removal');
      await blocked(f);
      restore.resolve();
      await until(() => !f.launcher.disabled);
      assert(
        document.activeElement === f.launcher,
        `${variant.name}: focus not restored`,
      );
      assert(
        f.calls.join(',') === 'occlude:true,occlude:false',
        'Duplicate transition commands',
      );
      f.action('diagnostics');
      await until(() => f.calls.length === 3);
      await f.open();
      await f.close();
      assert(
        f.occluded() && f.calls.at(-1) === 'occlude:true',
        'Diagnostics lost occlusion',
      );
      assert(
        document.activeElement === f.launcher,
        'Diagnostics close lost focus',
      );
      passed(
        `${variant.name}: delayed hide/restore excludes competing actions, restores launcher focus once and retains diagnostics`,
      );
    } finally {
      await f.dispose();
    }

    for (const phase of ['hide', 'restore'] as const) {
      const f = await fixture(variant);
      const reply = phase === 'hide' ? f.holdHide() : f.holdRestore();
      if (phase === 'hide') {
        f.launcher.focus();
        f.launcher.click();
      } else {
        await f.open();
        button(variant.close).click();
      }
      await until(() => f.calls.includes(`occlude:${phase === 'hide'}`));
      await f.dispose();
      const count = f.calls.length;
      reply.resolve();
      await settle();
      assert(
        f.calls.length === count && !f.dialog(),
        'Disposed transition continued',
      );
    }
    passed(
      `${variant.name}: disposal during hide and restore cannot revive presentation or send another command`,
    );

    for (const operation of ['draft', 'accept'] as const) {
      for (const dispose of [false, true]) {
        const f = await fixture(variant);
        let disposed = false;
        try {
          if (operation === 'draft') {
            const editor = host.querySelector<HTMLTextAreaElement>(
              '[aria-label="Subtitle text"]',
            )!;
            editor.value = 'Unapplied draft';
            editor.dispatchEvent(new Event('input', { bubbles: true }));
            await settle();
          }
          await f.open();
          if (operation === 'accept') {
            button(variant.preview).click();
            await until(() => !button(variant.accept).disabled);
          }
          const reply = f.holdMutation();
          button(
            operation === 'draft' ? 'Apply cue draft' : variant.accept,
          ).click();
          await until(() =>
            f.calls.includes(
              operation === 'draft' ? 'document:edit' : 'accept',
            ),
          );
          f.dialog()!.dispatchEvent(new Event('cancel', { cancelable: true }));
          button(variant.close).click();
          await settle();
          assert(
            f.dialog() && f.occluded(),
            'Mutation acknowledgment allowed close',
          );
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
              'Disposed mutation sent follow-up or notice',
            );
          else if (operation === 'accept') {
            await until(() => !f.dialog() && !f.launcher.disabled);
            assert(
              document.activeElement === f.launcher,
              'Acceptance lost focus',
            );
          } else await f.close();
        } finally {
          if (!disposed) await f.dispose();
        }
      }
    }
    passed(
      `${variant.name}: draft/accept acknowledgments block Escape/Close and disposal suppresses late presentation`,
    );
  }
  for (const phase of ['hide', 'restore'] as const) {
    const f = await fixture(variants[0]);
    try {
      const reply = phase === 'hide' ? f.holdHide() : f.holdRestore();
      if (phase === 'hide') {
        f.launcher.focus();
        f.launcher.click();
      } else {
        await f.open();
        button(variants[0].close).click();
      }
      await until(() => f.calls.includes(`occlude:${phase === 'hide'}`));
      reply.reject(new Error(`Injected ${phase} failure`));
      await until(
        () =>
          !f.launcher.disabled &&
          host.textContent!.includes(`Injected ${phase} failure`),
      );
      assert(!f.dialog(), 'Failed transition retained content');
      if (phase === 'restore')
        assert(
          document.activeElement === f.launcher,
          'Restore failure lost focus recovery',
        );
    } finally {
      await f.dispose();
    }
  }
  passed('Hide/restore failures report errors and recover workspace gates');
  {
    const f = await fixture(variants[0]);
    await f.open();
    const late = f.holdStart();
    button(variants[0].preview).click();
    await until(() => f.calls.includes('start'));
    await f.close();
    late.resolve();
    await until(() => f.cancels.includes(999));
    assert(!f.dialog(), 'Late admission revived dialog');
    await f.dispose();
    passed(
      'Closing during admission cancels the eventual job without reviving timing review',
    );
  }
  {
    const f = await fixture(variants[0], false);
    try {
      await f.open();
      await f.close();
      assert(
        !f.calls.length && document.activeElement === f.launcher,
        'Below-WebView path lost focus or sent occlusion',
      );
    } finally {
      await f.dispose();
    }
    passed(
      'Below-WebView capability needs no occlusion IPC and still restores focus',
    );
  }
  {
    const f = await fixture(variants[0]);
    try {
      await f.open();
      const reply = f.holdRestore();
      button(variants[0].close).click();
      await until(() => f.calls.includes('occlude:false'));
      f.launcher.remove();
      let focused = false;
      f.launcher.focus = () => {
        focused = true;
      };
      reply.resolve();
      await until(() => !f.launcher.disabled);
      assert(!focused, 'Close attempted to focus a detached launcher');
    } finally {
      await f.dispose();
    }
    passed('A launcher removed during restoration is not focused');
  }
  heading.textContent = `PASS: all ${output.children.length} timing modal checks`;
}
void run().catch((error) => {
  heading.textContent = `FAIL: ${String(error)}`;
});
