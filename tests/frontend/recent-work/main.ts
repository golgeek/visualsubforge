import { mount, tick, unmount } from 'svelte';
import { RecentWork } from '../../../src/lib/start/recent-work.svelte';
import RecentList from '../../../src/lib/start/RecentList.svelte';
import { WorkspaceOpen } from '../../../src/lib/start/workspace-open';
import { Workspace } from '../../../src/lib/workspace/workspace.svelte';
import type { RecentEntry, RecentRequest } from '../../../src/lib/client';
import '../../../src/lib/ui/tokens.css';
import '../../../src/lib/ui/base.css';
const host = document.querySelector<HTMLElement>('#test')!;
function assert(value: unknown, message: string) {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const item = document.createElement('li');
  item.textContent = 'PASS: ' + message;
  document.querySelector('#checks')!.append(item);
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
async function until(condition: () => boolean) {
  const deadline = performance.now() + 5000;
  while (!condition()) {
    assert(performance.now() < deadline, 'Recent work did not settle');
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await tick();
}
const entry: RecentEntry = {
  kind: 'subtitles',
  path: '/work/episode.srt',
  mediaPath: '/work/episode.unusual',
  subtitlePath: '/work/episode.srt',
  usedAtMs: 1700000000000,
  summary: {
    cues: 37,
    issues: 4,
    complete: false,
    profileName: 'Recorded profile',
    profileHash: 'a'.repeat(64),
    sourceSha256: 'b'.repeat(64),
    checkedAtMs: 1699999999000,
  },
};
const request: RecentRequest = {
  session: 1,
  generation: 2,
  revision: 3,
  projectToken: null,
};
async function run() {
  const lists: ReturnType<typeof deferred<RecentEntry[]>>[] = [];
  const captures: {
    request: RecentRequest;
    reply: ReturnType<typeof deferred<RecentEntry[]>>;
  }[] = [];
  const removals: Pick<RecentEntry, 'kind' | 'path'>[] = [];
  const recent = new RecentWork({
    listRecent: () => {
      const reply = deferred<RecentEntry[]>();
      lists.push(reply);
      return reply.promise;
    },
    captureRecent: (request) => {
      const reply = deferred<RecentEntry[]>();
      captures.push({ request, reply });
      return reply.promise;
    },
    forgetRecent: async (value) => {
      removals.push(value);
      return [];
    },
  });
  let opened: RecentEntry | null = null;
  const component = mount(RecentList, {
    target: host,
    props: {
      recent,
      disabled: false,
      onopen: async (value) => {
        opened = value;
      },
    },
  });
  try {
    const first = recent.load();
    const second = recent.load();
    lists[0]!.resolve([]);
    await first;
    assert(recent.loading, 'Older load cannot clear newer loading');
    lists[1]!.resolve([
      entry,
      { ...entry, kind: 'media', path: '/work/unknown.mkv', summary: null },
    ]);
    await second;
    await tick();
    assert(
      host.textContent?.includes('37 cues · 4 issues') &&
        host.textContent.includes('Last checked') &&
        host.textContent.includes('partial coverage') &&
        host.textContent.includes('Issue count unavailable'),
      'Historical/unknown counts are explicit',
    );
    host.querySelector<HTMLButtonElement>('button.open')!.click();
    await tick();
    assert(opened === entry, 'Open uses full stored metadata');
    passed(
      'Reload retains real historical counts, source identity, partial coverage and unknown counts',
    );
    recent.configure({ identity: 'old', request });
    await until(() => captures.length === 1);
    recent.configure({ identity: 'new', request: { ...request, revision: 4 } });
    captures[0]!.reply.resolve([{ ...entry, path: '/work/stale.srt' }]);
    await until(() => captures.length === 2);
    assert(
      recent.entries[0]!.path === entry.path &&
        captures[1]!.request.revision === 4,
      'Stale capture suppressed and newest context serialized',
    );
    captures[1]!.reply.resolve([{ ...entry, path: '/work/current.srt' }]);
    await until(() => recent.entries[0]!.path === '/work/current.srt');
    passed(
      'One capture runs at a time and obsolete replies cannot replace the list',
    );
    recent.configure({ identity: 'failure', request });
    await until(() => captures.length === 3);
    captures[2]!.reply.reject(new Error('Metadata write rejected'));
    await until(() => recent.error.includes('Metadata write rejected'));
    recent.configure({ identity: 'failure', request });
    await new Promise((resolve) => setTimeout(resolve, 600));
    assert(
      captures.length === 3 && recent.entries[0]!.path === '/work/current.srt',
      'No mutation replay or list loss on error',
    );
    passed(
      'Failed capture preserves the list and is never automatically replayed',
    );
    recent.configure({ identity: 'remove', request });
    await until(() => captures.length === 4);
    const removing = recent.forget(entry);
    assert(
      recent.removing && removals.length === 0,
      'Removal waits for pending capture',
    );
    captures[3]!.reply.resolve([entry]);
    await removing;
    await tick();
    assert(
      removals.length === 1 &&
        removals[0] === entry &&
        recent.entries.length === 0,
      'Only metadata removal requested after capture',
    );
    passed(
      'Removal waits for capture and only removes the requested metadata entry',
    );
    const late = recent.load();
    recent.dispose();
    lists[2]!.resolve([entry]);
    await late;
    await recent.load();
    recent.configure({ identity: 'disposed', request });
    assert(
      recent.entries.length === 0 && lists.length === 3,
      'Disposed list ignores late read and new work',
    );
    passed('Disposal suppresses late list publication and new requests');
  } finally {
    recent.dispose();
    await unmount(component);
  }
  const workspace = new Workspace();
  workspace.ready = true;
  const paths: string[] = [];
  let allow = false;
  const opener = new WorkspaceOpen(
    workspace,
    async (file) => {
      paths.push(file.path);
      if (!allow) return false;
      if (file.path.includes('missing'))
        throw new Error('File no longer exists');
      return true;
    },
    async () => {},
  );
  await opener.recent(entry);
  assert(
    paths.length === 1 && paths[0] === entry.path && !workspace.busy,
    'Cancellation stops paired media',
  );
  allow = true;
  paths.length = 0;
  await opener.recent(entry);
  assert(
    paths.length === 2 && paths[1] === entry.mediaPath,
    'Rust-classified recent media does not require extension guessing',
  );
  passed(
    'Recent pairs use ordinary protection and stop on cancellation before opening media',
  );
  await opener.recent({ ...entry, path: '/work/missing.srt' });
  assert(
    workspace.error.includes('File no longer exists') &&
      !workspace.busy &&
      paths.length === 3,
    'Failed open releases gate and stops pair',
  );
  opener.dispose();
  passed(
    'Failed recent open reports the error, releases the workspace gate and stops the pair',
  );
  const pending = deferred<RecentEntry[]>();
  let calls = 0;
  const disposed = new RecentWork({
    listRecent: async () => [],
    captureRecent: () => {
      ++calls;
      return pending.promise;
    },
    forgetRecent: async () => [],
  });
  disposed.configure({ identity: 'first', request });
  await until(() => calls === 1);
  disposed.configure({ identity: 'next', request });
  disposed.dispose();
  pending.resolve([entry]);
  await new Promise((resolve) => setTimeout(resolve, 600));
  assert(
    calls === 1 && disposed.entries.length === 0,
    'Disposed capture does not publish or schedule newest input',
  );
  passed(
    'Disposal during capture retires the queued context and its eventual reply',
  );
  document.querySelector('#result')!.textContent =
    'PASS: all 8 recent-work checks';
}
run().catch((error) => {
  document.querySelector('#result')!.textContent = 'FAIL: ' + String(error);
  console.error(error);
});
