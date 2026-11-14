import { runViewAction } from '../../../src/lib/workspace/view-actions';

/** Isolate admission ordering: feature owners are not used by this action. */
export async function pauseChecks(passed: (message: string) => void) {
  const events: string[] = [];
  let release!: (accepted: boolean) => void;
  let disposed = false;
  const owners = {
    gestures: { cancelHeld: () => events.push('cancel gesture') },
    workspace: {
      notice: '',
      get disposed() {
        return disposed;
      },
      task: async (action: () => Promise<void>) => action(),
    },
    media: {
      status: { loaded: true },
      command: async (action: { kind: string; paused: boolean }) => {
        if (action.kind !== 'pause' || action.paused !== true)
          throw new Error('Arrange must request pause');
        events.push('pause');
        return new Promise<boolean>((resolve) => (release = resolve));
      },
    },
    workspaceModal: {
      open: async (identity: string) => {
        if (identity !== 'arrange') throw new Error('Wrong modal');
        events.push('open');
      },
    },
  } as unknown as Parameters<typeof runViewAction>[1];
  for (const state of ['accepted', 'rejected', 'disposed'] as const) {
    events.length = 0;
    const opening = runViewAction('arrangePanels', owners);
    if (events.join(',') !== 'cancel gesture,pause')
      throw new Error('Modal appeared before pause acknowledgment');
    disposed = state === 'disposed';
    release(state !== 'rejected');
    await opening;
    if (events.includes('open') !== (state === 'accepted'))
      throw new Error('Pause rejection or disposal admitted arrange');
    passed(
      `loaded-media arrange ${state} obeys pause-before-presentation ordering`,
    );
  }
}
