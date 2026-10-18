import { mount, unmount, tick } from 'svelte';
import LegacyImportDialog from '../../../src/lib/LegacyImportDialog.svelte';
import { LegacyImportSession } from '../../../src/lib/legacy-import-session.svelte';
import type {
  FileChoice,
  LegacyImportRequest,
  LegacyImportReview,
} from '../../../src/lib/client';
import { legacyImportReview } from '../ipc-contract-samples';

const host = document.querySelector('#test')!;
const output = document.querySelector('#checks')!;
function assert(ok: unknown, message: string) {
  if (!ok) throw new Error(message);
}
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  output.append(row);
}
function button(label: string) {
  const found = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!found) throw new Error(`Missing ${label}`);
  return found;
}
async function until(check: () => boolean) {
  await tick();
  const end = performance.now() + 6000;
  while (!check()) {
    assert(performance.now() < end, `Timed out: ${host.textContent}`);
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
function review(): LegacyImportReview {
  return structuredClone(legacyImportReview);
}
function harness() {
  const state = {
    view: review(),
    cancels: [] as number[],
    publications: [] as string[],
    requests: [] as LegacyImportRequest[],
    choices: [] as { kind: FileChoice; current: string | null }[],
    reads: 0,
    choose: null as null | (() => Promise<string | null>),
    start: null as null | (() => Promise<number>),
    read: null as null | (() => Promise<LegacyImportReview>),
    publish: null as null | (() => Promise<void>),
  };
  const session = new LegacyImportSession({
    chooseFile: async (kind, current = null) => {
      state.choices.push({ kind, current });
      return state.choose ? state.choose() : '/fixture/selected.vssprj';
    },
    projectImportStart: async (request) => {
      state.requests.push(request);
      return state.start ? state.start() : 1;
    },
    projectImportView: async () => {
      state.reads++;
      return state.read ? state.read() : structuredClone(state.view);
    },
    projectImportCancel: async (id) => {
      state.cancels.push(id);
    },
    projectImportPublish: async (_, path) => {
      state.publications.push(path);
      if (state.publish) await state.publish();
      else state.view = { ...state.view, phase: 'published', outputPath: path };
    },
  });
  return { state, session };
}
async function lifecycle() {
  {
    const { state, session } = harness();
    await session.pick('path');
    const late = deferred<number>();
    state.start = () => late.promise;
    const starting = session.start();
    session.dispose();
    late.resolve(77);
    await starting;
    assert(
      state.cancels.includes(77) && !session.view,
      'Closed session adopted a late job',
    );
    passed('Unmount cancels a job returned after disposal');
  }
  {
    const { state, session } = harness();
    await session.pick('path');
    await session.start();
    const late = deferred<LegacyImportReview>();
    state.read = () => late.promise;
    const polling = session.poll();
    const reads = state.reads;
    await session.poll();
    assert(state.reads === reads, 'Concurrent status reads');
    state.choose = async () => '/relocated/字幕.vtt';
    await session.pick('subtitlePath');
    late.resolve(review());
    await polling;
    assert(
      !session.view && !session.canPublish && state.cancels.includes(1),
      'Old report survived relocation',
    );
    state.read = null;
    await session.start();
    assert(
      state.requests.at(-1)?.subtitlePath === '/relocated/字幕.vtt',
      'Relocation missing from request',
    );
    session.reset('subtitlePath');
    assert(
      !session.view && session.request.subtitlePath === null,
      'Reset retained old review',
    );
    await session.pick('mediaPath');
    await session.pick('path');
    assert(
      session.request.mediaPath === null &&
        session.request.subtitlePath === null,
      'New source inherited old relocations',
    );
    session.dispose();
    passed(
      'Single-flight reads and response epochs prevent old reviews from reviving after relocation or a new source',
    );
  }
  {
    const { state, session } = harness();
    await session.pick('path');
    await session.start();
    const late = deferred<string | null>();
    state.choose = () => late.promise;
    const publishing = session.publish();
    assert(
      !session.canClose && !session.canConfigure,
      'Picker lifetime not guarded',
    );
    session.dispose();
    late.resolve('/late.vsfproject');
    await publishing;
    assert(
      state.publications.length === 0,
      'Late picker published after disposal',
    );
    passed('A destination reply after unmount cannot create a file');
  }
  {
    const { state, session } = harness();
    await session.pick('path');
    await session.start();
    state.choose = async () => null;
    await session.pick('mediaPath');
    await session.publish();
    assert(
      session.canPublish && state.publications.length === 0,
      'Cancelled picker lost valid review',
    );
    assert(
      state.choices.at(-1)?.current === '/fixture/selected.imported.vsfproject',
      'Unsafe output suggestion',
    );
    state.choose = async () => '/new.vsfproject';
    state.publish = async () => {
      throw new Error('Dispatch rejected');
    };
    await session.publish();
    assert(
      session.canPublish &&
        session.canClose &&
        session.error.includes('Dispatch rejected'),
      'Confirmed rejection not retryable',
    );
    session.dispose();
    passed(
      'Picker cancellation retains review and confirmed dispatch failure permits retry',
    );
  }
  {
    const { state, session } = harness();
    await session.pick('path');
    await session.start();
    state.publish = async () => {
      throw new Error('Reply lost');
    };
    state.read = async () => {
      throw new Error('Status unavailable');
    };
    await session.publish();
    assert(
      session.running &&
        !session.canClose &&
        !session.canConfigure &&
        !session.canPublish,
      'Unknown publication outcome released guards',
    );
    const before = session.request;
    await session.pick('path');
    session.reset('mediaPath');
    assert(
      session.request === before,
      'Unknown publication allowed reconfiguration',
    );
    await session.cancel();
    assert(
      state.cancels.includes(1) && !session.canClose,
      'Uncertain publication lost cancellation/lifetime guard',
    );
    state.read = null;
    state.view = {
      ...state.view,
      phase: 'published',
      outputPath: '/completed.vsfproject',
    };
    await session.poll();
    await session.cancel();
    assert(
      session.canClose && session.view?.outputPath === '/completed.vsfproject',
      'Late cancellation hid completed receipt',
    );
    session.dispose();
    passed(
      'Lost publication replies prevent duplicates and retain a success receipt when publication wins cancellation',
    );
  }
  {
    const { state, session } = harness();
    await session.pick('path');
    state.view = {
      ...state.view,
      phase: 'preparing',
      running: true,
      report: null,
    };
    await session.start();
    assert(
      session.running && !session.canConfigure && !session.canPublish,
      'Preparing operation can be replaced',
    );
    state.view = { ...state.view, phase: 'cancelling' };
    await session.cancel();
    assert(!session.canStart, 'Draining worker allowed another review');
    state.view = { ...state.view, phase: 'cancelled', running: false };
    await session.poll();
    assert(session.canStart, 'Cancelled worker could not restart');
    state.view = review();
    await session.start();
    state.view = { ...state.view, phase: 'stale', error: 'Subtitles changed' };
    await session.poll();
    assert(
      !session.canPublish &&
        session.canStart &&
        session.error.includes('changed'),
      'Stale review exportable',
    );
    session.dispose();
    passed(
      'Preparing and cancelling workers prevent replacement; stale inputs require a new review',
    );
  }
}

let view = review();
let request: LegacyImportRequest | null = null;
let picked: string | null = '/fixture/old.vssprj';
let pickerKind: FileChoice | undefined;
let publications = 0;
let failOutput = false;
let closed: { path: string | undefined; open: boolean | undefined } | null =
  null;
function lastClose() {
  return closed;
}
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    invoke: async (
      command: string,
      args: { request?: LegacyImportRequest; kind?: FileChoice; path?: string },
    ) => {
      switch (command) {
        case 'choose_file':
          pickerKind = args.kind;
          return picked;
        case 'project_import_start':
          request = args.request!;
          view = review();
          if (!request.subtitlePath) {
            view.report!.canPublish = false;
            view.report!.subtitles.resolved = null;
            view.report!.subtitles.issue =
              'Subtitles are missing; choose the relocated file.';
          } else {
            view.report!.subtitles.overridden = true;
            view.report!.subtitles.resolved = request.subtitlePath;
          }
          view.report!.warnings.push({
            field: 'Unknown',
            line: null,
            value: '<img src=x onerror=alert(1)>',
            message: 'This setting is not imported.',
          });
          return 1;
        case 'project_import_view':
          return structuredClone(view);
        case 'project_import_cancel':
          if (view.phase !== 'published')
            view = {
              ...view,
              phase: 'cancelled',
              running: false,
              report: null,
            };
          return;
        case 'project_import_publish':
          publications++;
          view = failOutput
            ? {
                ...view,
                error: 'Output already exists; choose a new filename.',
              }
            : {
                ...view,
                phase: 'published',
                error: null,
                outputPath: args.path!,
              };
          return;
        default:
          throw new Error(`Unexpected IPC ${command}`);
      }
    },
  },
});
let app: ReturnType<typeof mount> | null = null;
async function run() {
  try {
    app = mount(LegacyImportDialog, {
      target: host,
      props: {
        onclose: (path, open) => {
          closed = { path, open };
        },
      },
    });
    await until(() => !!host.querySelector('dialog[open]'));
    assert(button('Review import').disabled, 'Empty source allowed review');
    button('Choose legacy project…').click();
    await until(() => !button('Review import').disabled);
    assert(pickerKind === 'legacyProject', 'Wrong project picker');
    button('Review import').click();
    await until(() => host.textContent!.includes('Subtitles are missing'));
    assert(
      button('Create new VSF project…').disabled && publications === 0,
      'Missing reference allowed publication',
    );
    assert(
      host.textContent!.includes('<img src=x onerror=alert(1)>') &&
        !host.querySelector('img'),
      'Untrusted warning interpreted as HTML',
    );
    passed(
      'Missing files block export; source paths and unsupported settings are escaped text',
    );
    picked = '/new/字幕.ass';
    button('Locate subtitles…').click();
    await until(
      () => !host.querySelector('[aria-label="Legacy import review"]'),
    );
    assert(pickerKind === 'subtitles', 'Wrong relocation picker');
    button('Review import').click();
    await until(
      () => !!host.querySelector('[aria-label="Legacy import review"]'),
    );
    assert(
      !button('Create new VSF project…').disabled &&
        host.textContent!.includes('relocated') &&
        host.textContent!.includes('Balanced'),
      'Relocated/default-policy review missing',
    );
    picked = null;
    button('Create new VSF project…').click();
    await until(() => !button('Create new VSF project…').disabled);
    assert(
      publications === 0 && pickerKind === 'saveProject',
      'Cancelled output choice published',
    );
    picked = '/new/copy.vsfproject';
    failOutput = true;
    button('Create new VSF project…').click();
    await until(() => host.textContent!.includes('Output already exists'));
    assert(
      !button('Create new VSF project…').disabled,
      'Failed destination lost review',
    );
    failOutput = false;
    button('Create new VSF project…').click();
    await until(() => !!host.querySelector('[aria-label="Project created"]'));
    assert(closed === null, 'Export automatically opened project');
    button('Open created project').click();
    assert(
      closed?.path === picked && closed.open === true,
      'Explicit Open lost output path',
    );
    passed(
      'Relocation needs a new review; output cancellation/failure is retryable and opening requires an explicit action',
    );
    await unmount(app);
    app = null;
    closed = null;
    app = mount(LegacyImportDialog, {
      target: host,
      props: {
        canOpen: false,
        onclose: (path, open) => {
          closed = { path, open };
        },
      },
    });
    await until(() => !!host.querySelector('dialog[open]'));
    picked = '/fixture/other.vssprj';
    button('Choose legacy project…').click();
    await until(() => !button('Review import').disabled);
    picked = '/new/字幕.ass';
    button('Locate subtitles…').click();
    await until(() => !button('Review import').disabled);
    button('Review import').click();
    await until(
      () => !!host.querySelector('[aria-label="Legacy import review"]'),
    );
    picked = '/new/other.vsfproject';
    button('Create new VSF project…').click();
    await until(() => !!host.querySelector('[aria-label="Project created"]'));
    assert(
      button('Open created project').disabled &&
        host.textContent!.includes('Media playback is unavailable'),
      'Unavailable playback offered project restoration',
    );
    button('Close import').click();
    assert(
      lastClose()?.path === picked && lastClose()?.open === false,
      'Non-playback build lost output receipt',
    );
    await unmount(app);
    app = null;
    passed(
      'A build without playback can create and retain the project while explaining why Open is unavailable',
    );
    await lifecycle();
    document.querySelector('#result')!.textContent =
      `PASS: all ${output.querySelectorAll('li').length} legacy import checks`;
  } catch (error) {
    document.querySelector('#result')!.textContent = `FAIL: ${String(error)}`;
    console.error(error);
  } finally {
    if (app) await unmount(app);
  }
}
void run();
