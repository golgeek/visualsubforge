import { openingCases } from './owners';
import { mount, tick, unmount } from 'svelte';
import App from '../../../src/App.svelte';
import {
  defaultPreferences,
  emptyDocument,
  emptyStatus,
  type DocumentAction,
  type DocumentSnapshot,
  type MediaAction,
  type RecoveryStatus,
} from '../../../src/lib/client';
import { emptyQuality } from '../empty-quality';
const host = document.querySelector<HTMLElement>('#test')!;
let snapshot: DocumentSnapshot = structuredClone(emptyDocument);
let path: string | null = '/captions.srt';
let opened = 0;
let recovered = 0;
let occluded = false;
const callbacks = new Map<number, (value: unknown) => void>();
const events = new Map<string, number>();
const recovery: RecoveryStatus = {
  entries: [
    {
      id: 'recover-1',
      sourcePath: '/original.srt',
      capturedMs: null,
      revision: 1,
      error: null,
    },
  ],
  error: null,
};
function accepted(source: string | null, dirty = false): DocumentSnapshot {
  return {
    ...emptyDocument,
    loaded: true,
    generation: snapshot.generation + 1,
    revision: 0,
    path: source,
    dirty,
    cues: [{ id: 1, startMs: 0, endMs: 2000, text: 'Accepted text' }],
  };
}
Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
  configurable: true,
  value: { unregisterListener: () => {} },
});
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
        handler?: number;
        event?: string;
        kind?: string;
        id?: string;
      },
    ) {
      switch (command) {
        case 'plugin:event|listen':
          events.set(args.event!, args.handler!);
          return args.handler;
        case 'plugin:event|unlisten':
        case 'update_menu':
          return;
        case 'preferences_get':
          return {
            ...structuredClone(defaultPreferences),
            appearance: {
              ...defaultPreferences.appearance,
              theme:
                new URLSearchParams(location.search).get('theme') === 'light'
                  ? 'light'
                  : 'dark',
            },
          };
        case 'quality_read':
          return emptyQuality(args);
        case 'quality_cancel':
          return null;
        case 'quality_settings':
          return {
            current: {
              epoch: 1,
              profile: defaultPreferences.quality,
              timing: defaultPreferences.timing,
            },
            builtins: [defaultPreferences.quality],
          };
        case 'language_words':
          return {
            epoch: 1,
            personal: { englishUs: [], french: [] },
            ignored: { englishUs: [], french: [] },
          };
        case 'recovery_status':
          return structuredClone(recovery);
        case 'recovery_discard':
          recovery.entries = recovery.entries.filter(
            (entry) => entry.id !== args.id,
          );
          return;
        case 'choose_file':
          if (args.kind !== 'workspace')
            throw new Error('Expected unified Open picker');
          return path;
        case 'media_action':
          if (args.action?.kind === 'occlude') occluded = args.action.occluded;
          return { ...emptyStatus, available: true, surfaceAboveWebview: true };
        case 'document_action': {
          const action = args.action as DocumentAction;
          if (action.kind === 'create') {
            snapshot = accepted(null, true);
            snapshot.cues[0]!.text = '';
          }
          if (action.kind === 'open') {
            ++opened;
            snapshot = accepted(action.path);
          }
          if (action.kind === 'close')
            snapshot = {
              ...emptyDocument,
              generation: snapshot.generation + 1,
            };
          if (action.kind === 'recover') {
            ++recovered;
            snapshot = { ...accepted(null, true), requiresSaveAs: true };
          }
          return structuredClone(snapshot);
        }
        default:
          throw new Error('Unexpected command: ' + command);
      }
    },
  },
});
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const item = document.createElement('li');
  item.textContent = 'PASS: ' + message;
  document.querySelector('#checks')!.append(item);
}
function button(label: string) {
  return [...host.querySelectorAll('button')].find(
    (item) => item.textContent?.trim() === label,
  )!;
}
async function until(condition: () => boolean) {
  const deadline = performance.now() + 5000;
  while (!condition()) {
    assert(
      performance.now() < deadline,
      'Start screen timed out: ' + host.textContent,
    );
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await tick();
}
function drop(paths: string[]) {
  callbacks.get(events.get('tauri://drag-drop')!)!({ payload: { paths } });
}
async function run() {
  await openingCases(passed);
  let app = mount(App, { target: host });
  const reset = async () => {
    await unmount(app);
    snapshot = structuredClone(emptyDocument);
    events.clear();
    app = mount(App, { target: host });
    await until(
      () =>
        !!button('New subtitles') &&
        !button('New subtitles').disabled &&
        events.has('tauri://drag-drop'),
    );
  };
  try {
    await until(
      () =>
        !!button('New subtitles') &&
        !button('New subtitles').disabled &&
        events.has('tauri://drag-drop'),
    );
    assert(
      host.querySelector('.workspace')!.hasAttribute('inert'),
      'Empty editor is inert while its owners remain mounted',
    );
    assert(
      getComputedStyle(host.querySelector('.video-space')!).boxShadow !==
        'none',
      'Mounted placeholder retains the window backdrop',
    );
    const editor = host.querySelector('[aria-label="Subtitle text"]');
    button('New subtitles').click();
    await until(() => !host.querySelector('.start-screen'));
    assert(
      snapshot.loaded &&
        snapshot.path === null &&
        snapshot.dirty &&
        snapshot.cues.length === 1,
      'New subtitles uses accepted create command',
    );
    assert(
      host.querySelector('[aria-label="Subtitle text"]') === editor,
      'Editor remains mounted across start transition',
    );
    passed(
      'New subtitles leaves Start through the existing Rust-owned create flow without recreating owners',
    );
    await reset();
    button('Open…').click();
    await until(() => opened === 1 && !host.querySelector('.start-screen'));
    assert(
      snapshot.path === '/captions.srt',
      'Unified picker classifies subtitles',
    );
    passed(
      'Open chooses and dispatches a subtitle file through the ordinary owner',
    );
    await reset();
    drop(['/dropped.ASS']);
    await until(() => opened === 2 && snapshot.path === '/dropped.ASS');
    assert(
      !host.querySelector('.start-screen'),
      'Native subtitle drop enters workspace',
    );
    drop(['/one.srt', '/two.srt']);
    await until(() => !!host.textContent?.includes('pair one video/audio'));
    assert(opened === 2, 'Ambiguous drop does not mutate accepted state');
    passed(
      'Native file drop classifies subtitles and rejects ambiguous groups before opening',
    );
    await reset();
    drop(['/legacy.vssprj']);
    await until(() => !!host.querySelector('#legacy-import-title'));
    assert(
      occluded && host.textContent?.includes('/legacy.vssprj'),
      'Dropped legacy path enters hidden-video review',
    );
    button('Close import').click();
    await until(() => !host.querySelector('dialog[open]'));
    button('Import VisualSubSync project').click();
    await until(() => !!host.querySelector('#legacy-import-title'));
    assert(
      host.textContent?.includes('No project selected'),
      'Import action retains explicit picker review',
    );
    passed(
      'Legacy drops preselect the import review; the start action opens the existing import workflow',
    );
    await reset();
    await until(() => !!button('Recover'));
    button('Recover').click();
    await until(() => recovered === 1 && !host.querySelector('.start-screen'));
    assert(
      snapshot.path === null &&
        snapshot.requiresSaveAs &&
        recovery.entries.length === 1,
      'Recovery is an unsaved copy and retains its original snapshot',
    );
    button('Recovery').click();
    await until(() => !!button('Close recovery'));
    assert(
      occluded &&
        !!host.querySelector('dialog[aria-label="Subtitle recovery"]'),
      'Loaded workspace recovery uses HideVideo',
    );
    button('Discard…').click();
    await tick();
    assert(recovery.entries.length === 1, 'Discard needs confirmation');
    button('Cancel').click();
    await tick();
    button('Close recovery').click();
    await until(() => !host.querySelector('dialog[open]') && !occluded);
    passed(
      'Recovery card opens an unsaved copy and remains available in a protected dialog with discard confirmation',
    );
    await reset();
    path = null;
    button('Open…').click();
    await tick();
    await until(() => !button('Open…').disabled);
    assert(
      !!host.querySelector('.start-screen') && opened === 2,
      'Picker cancellation preserves start',
    );
    const late = callbacks.get(events.get('tauri://drag-drop')!)!;
    await unmount(app);
    late({ payload: { paths: ['/late.srt'] } });
    await tick();
    assert(opened === 2, 'Disposed native listener cannot reopen document');
    passed(
      'Picker cancellation and late native drops leave accepted state unchanged',
    );
    document.querySelector('#result')!.textContent =
      'PASS: all 9 start screen checks';
  } finally {
    if (host.childElementCount) await unmount(app);
  }
}
if (new URLSearchParams(location.search).has('preview')) {
  document.querySelector<HTMLElement>('#result')!.hidden = true;
  document.querySelector<HTMLElement>('#checks')!.hidden = true;
  mount(App, { target: host });
} else
  run().catch((error) => {
    document.querySelector('#result')!.textContent = 'FAIL: ' + String(error);
    console.error(error);
  });
