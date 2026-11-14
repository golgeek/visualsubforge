import { Workspace } from '../../../src/lib/workspace/workspace.svelte';
import { WorkspaceOpen } from '../../../src/lib/start/workspace-open';
export async function openingCases(passed: (message: string) => void) {
  const assert = (value: unknown, message: string) => {
    if (!value) throw new Error(message);
  };
  const workspace = new Workspace();
  workspace.ready = true;
  const opened: string[] = [];
  let accept = false;
  let stopped = 0;
  let releaseListen!: (stop: () => void) => void;
  let releasePicker!: (path: string | null) => void;
  const opener = new WorkspaceOpen(
    workspace,
    async (file) => {
      opened.push(file.kind);
      return accept;
    },
    async () => {},
    {
      chooseFile: () =>
        new Promise<string | null>((resolve) => {
          releasePicker = resolve;
        }),
      listenFileDrop: async () =>
        new Promise<() => void>((resolve) => {
          releaseListen = resolve;
        }),
    },
  );
  await opener.paths(['/video.mkv', '/subtitles.srt']);
  assert(
    opened.join(',') === 'subtitles',
    'Cancelled subtitle protection prevents media open',
  );
  accept = true;
  opened.length = 0;
  await opener.paths(['/video.mkv', '/subtitles.srt']);
  assert(
    opened.join(',') === 'subtitles,media',
    'Accepted pair dispatches in protected order',
  );
  passed(
    'Paired drops resolve subtitle protection first and stop on cancellation',
  );
  opened.length = 0;
  await opener.paths(['/project.vsfproject']);
  await opener.paths(['/old.vssprj']);
  assert(
    opened.join(',') === 'project,legacy',
    'Projects dispatch to the correct existing owner',
  );
  passed('VSF and VisualSubSync paths keep separate opening owners');
  opened.length = 0;
  const picking = opener.pick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert(workspace.busy, 'Picker holds workspace gate');
  await opener.paths(['/ignored.srt']);
  assert(
    opened.length === 0,
    'A drop cannot replace a pending picker operation',
  );
  const listening = opener.startListening();
  opener.dispose();
  releasePicker('/late.srt');
  releaseListen(() => {
    ++stopped;
  });
  await Promise.all([picking, listening]);
  assert(
    opened.length === 0 && stopped === 1 && !workspace.busy,
    'Late picker/listener responses retire after disposal',
  );
  workspace.dispose();
  passed(
    'Pending picker gates drops, and disposal retires late paths and subscriptions',
  );
}
