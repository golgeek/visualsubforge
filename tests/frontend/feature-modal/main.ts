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
import { statisticsView } from '../ipc-contract-samples';
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
function button(label: string, root: ParentNode = host) {
  const result = [...root.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  assert(result, `Missing button: ${label}`);
  return result!;
}
const variants = [
  { name: 'settings', close: 'Cancel', action: 'settings' },
  { name: 'resync', close: 'Close', action: 'resync' },
  { name: 'statistics', close: 'Close statistics', action: null },
  { name: 'text', close: 'Close text tools', action: null },
] as const;
async function fixture(variant: (typeof variants)[number], above = true) {
  const calls: string[] = [];

  let occluded = false;
  let hide: ReturnType<typeof deferred> | null = null;
  let restore: ReturnType<typeof deferred> | null = null;

  let handler = 0;
  const callbacks = new Map<number, (event: unknown) => void>();
  let doc = {
    ...emptyDocument,
    loaded: true,
    generation: statisticsView.key.generation,
    revision: statisticsView.key.revision,
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
              loaded: true,
            };
          }
          case 'document_action': {
            const action = args.action as DocumentAction;
            if (action.kind !== 'status') {
              calls.push(`document:${action.kind}`);

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
        if (command === 'statistics_start') return 999;
        if (command === 'statistics_view')
          return { ...structuredClone(statisticsView), id: 999 };
        if (command === 'statistics_cancel') return;
        if (command === 'quality_settings')
          return {
            current: {
              epoch: 1,
              profile: defaultPreferences.quality,
              timing: defaultPreferences.timing,
            },
            builtins: [defaultPreferences.quality],
          };
        if (command === 'language_words')
          return {
            epoch: 1,
            personal: defaultPreferences.personalWords,
            ignored: { englishUs: [], french: [] },
          };
        calls.push(command);
        throw new Error(`Unexpected IPC: ${command}`);
      },
    },
  });
  const app = mount(App, { target: host });
  await until(
    () =>
      !!callbacks.get(handler) &&
      [...host.querySelectorAll('button')].some(
        (b) =>
          b.textContent?.trim() ===
            (variant.name === 'statistics'
              ? 'Statistics…'
              : 'Find / Replace…') && !b.disabled,
      ),
  );
  // Media polling has started synchronously on mount; allow its projection to render.
  await settle();
  calls.length = 0;
  const launcher = button(
    variant.name === 'statistics' ? 'Statistics…' : 'Find / Replace…',
  );
  const dialog = () => host.querySelector<HTMLDialogElement>('dialog[open]');
  const action = (value: WorkspaceAction) =>
    callbacks.get(handler)!({ payload: value });
  return {
    calls,
    launcher,
    dialog,
    action,
    occluded: () => occluded,
    holdHide: () => (hide = deferred()),
    holdRestore: () => (restore = deferred()),
    begin: () => {
      launcher.focus();
      if (variant.action) action(variant.action);
      else launcher.click();
    },
    open: async () => {
      launcher.focus();
      if (variant.action) action(variant.action);
      else launcher.click();
      await until(() => !!dialog());
    },
    close: async () => {
      button(variant.close, host.querySelector('dialog[open]')!).click();
      await until(() => !dialog() && !launcher.disabled);
    },
    dispose: () => unmount(app),
  };
}
async function run() {
  for (const variant of variants) {
    const f = await fixture(variant);
    try {
      const hide = f.holdHide();
      f.begin();
      await until(() => f.calls.includes('occlude:true'));
      assert(!f.dialog(), 'Dialog mounted before hide acknowledgment');
      f.action('settings');
      f.action('openProject');
      assert(f.launcher.disabled, 'Workspace admitted during hide');
      hide.resolve();
      await until(() => !!f.dialog());
      const restore = f.holdRestore();
      button(variant.close, host.querySelector('dialog[open]')!).click();
      await until(() => f.calls.includes('occlude:false'));
      assert(
        !f.dialog() && f.launcher.disabled,
        `Close released workspace before restoration: ${variant.name}, dialog=${f.dialog()?.outerHTML.slice(0, 300)}, disabled=${f.launcher.disabled}, calls=${f.calls.join(',')}`,
      );
      restore.resolve();
      await until(() => !f.launcher.disabled);
      assert(document.activeElement === f.launcher, 'Return focus missing');
      passed(`${variant.name}: acknowledged hide/restore and focus ordering`);
    } finally {
      await f.dispose();
    }
    for (const phase of ['hide', 'restore'] as const) {
      const f = await fixture(variant);
      const reply = phase === 'hide' ? f.holdHide() : f.holdRestore();
      if (phase === 'hide') f.begin();
      else {
        await f.open();
        button(variant.close, host.querySelector('dialog[open]')!).click();
      }
      await until(() => f.calls.includes(`occlude:${phase === 'hide'}`));
      await f.dispose();
      const count = f.calls.length;
      reply.resolve();
      await settle();
      assert(
        !f.dialog() && f.calls.length === count,
        'Disposed modal continued',
      );
      passed(`${variant.name}: disposal during ${phase}`);
    }
  }
  heading.textContent = `PASS: all ${output.children.length} feature modal checks`;
}
void run().catch((error) => {
  heading.textContent = `FAIL: ${String(error)}`;
});
