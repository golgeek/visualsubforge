import { ExtractionSession } from '../../../src/lib/extraction/extraction-session.svelte';
import { DocumentSession } from '../../../src/lib/document/document-session.svelte';
import { Workspace } from '../../../src/lib/workspace/workspace.svelte';
import { WorkspaceModal } from '../../../src/lib/dialogs/workspace-modal.svelte';
import { emptyQuality } from '../empty-quality';
import type { updateMenu } from '../../../src/lib/client';
import { defaultPreferences } from '../../../src/lib/client';
import { mount, tick, unmount } from 'svelte';
import { isTauri } from '@tauri-apps/api/core';
import App from '../../../src/App.svelte';
import {
  emptyDocument,
  emptyStatus,
  type DocumentAction,
  type ExtractionAction,
  type ExtractionReply,
  type DocumentSnapshot,
  type MediaAction,
} from '../../../src/lib/client';

// Browser-only integration fixture: real App/client, modeled IPC, no disk writes.
if (isTauri()) throw new Error('Run this fixture in an ordinary browser.');
const host = document.getElementById('test')!;
const heading = document.getElementById('result')!;
const output = document.getElementById('checks')!;
let snapshot: DocumentSnapshot = structuredClone(emptyDocument);
const destination: string | null = null;
const failSave = false;
let pickers = 0;
let saves = 0;
const media = {
  ...emptyStatus,
  available: true,
  loaded: true,
  session: 1,
  durationSeconds: 100,
  positionSeconds: 5,
};
const transport: string[] = [];
function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}
function passed(message: string) {
  const item = document.createElement('li');
  item.textContent = `PASS: ${message}`;
  output.append(item);
}
const callbacks = new Map<number, (value: unknown) => void>();
let eventHandler = 0;
let menuState: Parameters<typeof updateMenu>[0] | undefined;
function menuAction(action: string) {
  callbacks.get(eventHandler)!({ payload: action });
}
function button(label: string) {
  const found = [
    ...host.querySelectorAll<HTMLButtonElement>(
      'button:not(dialog:not([open]) button)',
    ),
  ].find((b) => b.textContent?.trim() === label);
  if (!found) throw new Error(`Missing ${label}`);
  return found;
}
async function until(condition: () => boolean) {
  const deadline = performance.now() + 5000;
  while (!condition()) {
    if (performance.now() > deadline)
      throw new Error(
        `UI response timeout: starts=${starts}; ${host.textContent}`,
      );
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  await tick();
}
Object.defineProperty(window, '__TAURI_EVENT_PLUGIN_INTERNALS__', {
  configurable: true,
  value: { unregisterListener: () => {} },
});
Object.defineProperty(window, 'isTauri', { configurable: true, value: true });
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    transformCallback: (callback: (value: unknown) => void) => {
      const id = callbacks.size + 1;
      callbacks.set(id, callback);
      return id;
    },
    unregisterCallback: () => {},
    async invoke(
      command: string,
      args: {
        action?: DocumentAction | MediaAction | ExtractionAction;
        current?: string;
        kind?: string;
        handler?: number;
        event?: string;
        state?: Parameters<typeof updateMenu>[0];
      },
    ) {
      if (command === 'preferences_get')
        return structuredClone(defaultPreferences);
      switch (command) {
        case 'recent_list':
        case 'recent_capture':
        case 'recent_forget':
          return [];
        case 'quality_read':
          return emptyQuality(args);
        case 'scenes_start':
          return 1;
        case 'scenes_view':
          return { generation: 1, status: 'idle', cuts: [], complete: false };
        case 'quality_cancel':
          return null;
        case 'plugin:event|listen':
          if (args.event !== 'workspace-action') return 2;
          eventHandler = args.handler!;
          return 1;
        case 'plugin:event|unlisten':
          return;
        case 'update_menu':
          menuState = args.state;
          return;
        case 'recovery_status':
          return { entries: [], error: null };
        case 'media_action':
          if (args.action?.kind !== 'status' && args.action?.kind !== 'layout')
            transport.push(args.action!.kind);
          return structuredClone(media);
        case 'choose_file':
          if (args.kind === 'media') return '/synthetic/video.mkv';
          assert(args.kind === 'saveSubtitles', 'Unexpected picker');
          assert(
            args.current === 'video.fr.srt',
            'Suggested extraction name missing',
          );
          pickers++;
          return destination;
        case 'extraction_action': {
          const action = args.action as ExtractionAction;
          if (action.kind === 'start') {
            starts++;
            reply = {
              ...reply,
              listing: null,
              status: hold ? 'running' : 'ready',
              cueCount: 1,
            };
          }
          if (action.kind === 'list')
            reply = { ...reply, listing, status: 'ready' };
          if (action.kind === 'cancel')
            reply = { ...reply, status: 'cancelled' };
          if (action.kind === 'accept') {
            snapshot = {
              ...emptyDocument,
              generation: snapshot.generation + 1,
              loaded: true,
              dirty: true,
              requiresSaveAs: true,
              extractedFrom: reply.provenance!,
              cues: [{ id: 1, startMs: 1000, endMs: 2000, text: 'Extracted' }],
            };
            reply = { ...reply, snapshot, status: 'accepted' };
          }
          return structuredClone(reply);
        }
        case 'document_action': {
          const action = args.action as DocumentAction;
          if (action.kind === 'create') {
            assert(
              !snapshot.loaded && action.afterId === null,
              'Expected first cue',
            );
            snapshot = {
              ...snapshot,
              generation: 1,
              revision: 1,
              loaded: true,
              dirty: true,
              requiresSaveAs: true,
              canUndo: true,
              cues: [
                {
                  id: 1,
                  startMs: action.startMs,
                  endMs: action.endMs,
                  text: action.text,
                },
              ],
            };
          } else if (action.kind === 'save') {
            saves++;
            if (failSave) throw new Error('Disposable save failure');
            snapshot = {
              ...snapshot,
              path: action.path ?? snapshot.path,
              dirty: false,
              requiresSaveAs: false,
            };
          } else
            assert(
              action.kind === 'status',
              `Unexpected document action: ${action.kind}`,
            );
          return structuredClone(snapshot);
        }
        default:
          throw new Error(`Unexpected IPC: ${command}`);
      }
    },
  },
});

let starts = 0,
  hold = false;
const listing = {
  font_count: 1,
  tracks: [
    {
      index: 2,
      codec: 'subrip',
      language: 'fra',
      title: 'French',
      default: true,
      forced: false,
      hearing_impaired: false,
      unavailable: null,
    },
    {
      index: 3,
      codec: 'hdmv_pgs_subtitle',
      language: 'eng',
      title: 'Picture',
      default: false,
      forced: false,
      hearing_impaired: false,
      unavailable: 'Picture-based: needs OCR',
    },
  ],
};
let reply: ExtractionReply = {
  id: 1,
  status: 'ready',
  listing,
  error: null,
  snapshot: null,
  provenance: {
    media_path: '/synthetic/video.mkv',
    stream_index: 2,
    codec: 'subrip',
    language: 'fra',
    title: 'French',
    suggested_name: 'video.fr.srt',
  },
  cueCount: 1,
  fontCount: 1,
  repairedCues: [],
};
async function run() {
  const app = mount(App, { target: host });
  try {
    await until(() => !!menuState?.enabled.includes('extractSubtitles'));
    menuAction('extractSubtitles');
    await until(() => !!host.querySelector('dialog[open] input[type=radio]'));
    assert(
      host.textContent?.includes('Picture-based: needs OCR') === true,
      'Missing disabled reason',
    );
    assert(
      host.querySelectorAll('input[type=radio]:disabled').length === 1,
      'Picture track enabled',
    );
    passed(
      'chooser preserves stream order, default and unavailable picture reason',
    );
    button('Extract').click();
    await until(() => snapshot.loaded && !host.querySelector('dialog[open]'));
    assert(
      snapshot.dirty && snapshot.requiresSaveAs && snapshot.path === null,
      'Extraction must require Save As',
    );
    assert(
      host.textContent?.includes('Extracted 1 cue from track 2') === true,
      'Result notice missing',
    );
    passed(
      'accepted extraction is dirty and requires Save As with provenance notice',
    );
    menuAction('save');
    await until(() => pickers === 1);
    assert(saves === 0, 'Cancelled Save As wrote the document');
    passed(
      'Save As receives video.fr.srt and cancelled picker preserves document',
    );
    await until(() => !!menuState?.enabled.includes('extractSubtitles'));
    menuAction('extractSubtitles');
    await until(() => !!host.querySelector('dialog[open] input[type=radio]'));
    button('Extract').click();
    await until(
      () =>
        !!host.querySelector(
          'dialog[open][aria-label="Unsaved subtitle changes"]',
        ),
    );
    assert(starts === 1, 'Extraction ran before unsaved-work protection');
    button('Cancel').click();
    await until(() => !host.querySelector('dialog[open]'));
    assert(
      starts === 1 && snapshot.cues[0]?.text === 'Extracted',
      'Cancel changed document',
    );
    passed(
      'unsaved-work Cancel prevents extraction and preserves accepted document',
    );
    await until(() => !!menuState?.enabled.includes('extractSubtitles'));
    menuAction('extractSubtitles');
    await until(() => !!host.querySelector('dialog[open] input[type=radio]'));
    hold = true;
    button('Extract').click();
    await until(
      () =>
        !!host.querySelector(
          'dialog[open][aria-label="Unsaved subtitle changes"]',
        ),
    );
    button('Discard changes').click();
    await until(() => starts === 2);
    button('Cancel').click();
    await until(() => !host.querySelector('dialog[open]'));
    assert(
      snapshot.generation === 1 && snapshot.dirty,
      'Cancellation adopted partial document',
    );
    passed('mid-extraction Cancel preserves accepted state');
    const owner = new DocumentSession();
    const workspace = new Workspace();
    const modal = new WorkspaceModal(workspace, {
      occlude: async () => {},
      remainingOcclusion: () => false,
      afterRender: async () => {},
      captureFocus: () => () => {},
    });
    const session = new ExtractionSession(
      workspace,
      owner,
      modal,
      async () => true,
      async () => {},
    );
    await session.observe('/synthetic/notice.mkv', false);
    assert(session.notice?.count === 2, 'Usable media notice missing');
    session.dismiss();
    await session.observe('/synthetic/notice.mkv', false);
    assert(session.notice === null, 'Dismissed notice repeated');
    await session.observe('/synthetic/loaded.mkv', true);
    assert(session.notice === null, 'Notice shown with document open');
    session.dispose();
    owner.dispose();
    passed(
      'Media notice appears without a document and dismissal lasts for this session',
    );
    heading.textContent = `PASS: all ${output.children.length} extraction checks`;
  } finally {
    await unmount(app);
  }
}
void run().catch((error) => {
  heading.textContent = `FAIL: ${String(error)}`;
  console.error(error);
});
