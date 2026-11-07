import { WorkspaceModal } from '../../../src/lib/workspace-modal.svelte';

export async function modalCases(passed: (message: string) => void) {
  const assert = (value: unknown, message: string) => {
    if (!value) throw new Error(message);
  };
  function deferred() {
    let resolve!: () => void;
    let reject!: (error: Error) => void;
    const promise = new Promise<void>((yes, no) => {
      resolve = yes;
      reject = no;
    });
    return { promise, resolve, reject };
  }
  function fixture() {
    const events: string[] = [];
    let hide: ReturnType<typeof deferred> | null = null;
    let restore: ReturnType<typeof deferred> | null = null;
    let diagnostics = false;
    const modal = new WorkspaceModal({
      occlude: async (value) => {
        events.push(`occlude:${value}`);
        await (value ? hide?.promise : restore?.promise);
      },
      remainingOcclusion: () => diagnostics,
      captureFocus: () => () => {
        events.push('focus');
      },
      afterRender: async () => {
        events.push(`render:${modal.active}`);
      },
      reportError: (error) => {
        events.push(String(error));
      },
    });
    return {
      modal,
      events,
      holdHide: () => (hide = deferred()),
      holdRestore: () => (restore = deferred()),
      diagnostics: () => {
        diagnostics = true;
      },
    };
  }
  for (const kind of ['styles', 'fonts', 'conversion', 'plugins'] as const) {
    const f = fixture();
    const hide = f.holdHide();
    const opening = f.modal.open(kind);
    assert(
      f.modal.opening && f.modal.active && !f.modal.identity,
      'Dialog visible before hide acknowledgment',
    );
    await f.modal.open('plugins');
    assert(f.events.length === 1, 'Duplicate admission hid twice');
    hide.resolve();
    await opening;
    assert(
      f.modal.identity === kind && !f.modal.opening,
      'Wrong dialog opened',
    );
    const restore = f.holdRestore();
    const closing = f.modal.close();
    await Promise.resolve();
    await Promise.resolve();
    assert(
      !f.modal.identity && f.modal.active && !f.events.includes('focus'),
      'Closing released gate/focus before restore',
    );
    restore.resolve();
    await closing;
    assert(
      f.events.join(',') ===
        'occlude:true,render:true,occlude:false,render:false,focus',
      'Hide/remove/restore/enable/focus order changed',
    );
    assert(!f.modal.active, 'Close kept gate');
    f.modal.dispose();
  }
  passed(
    'All authoring identities hide before mounting and restore video before enabling/focusing the workspace',
  );
  {
    const f = fixture();
    const hide = f.holdHide();
    const opening = f.modal.open('styles');
    const closing = f.modal.close();
    assert(f.events.length === 1, 'Close raced pending hide');
    hide.resolve();
    await opening;
    await closing;
    assert(
      !f.modal.identity && f.events.includes('occlude:false'),
      'Delayed open revived a closed dialog',
    );
    await f.modal.open('fonts');
    assert(f.modal.identity === 'fonts', 'Close prevented reopening');
    f.diagnostics();
    await f.modal.close();
    assert(f.events.at(-3) === 'occlude:true', 'Diagnostics lost occlusion');
    f.modal.dispose();
  }
  passed(
    'Closing a delayed open waits for hide, prevents revival, permits reopening and preserves diagnostics occlusion',
  );
  for (const point of ['open', 'close'] as const) {
    const f = fixture();
    let pending: Promise<void>;
    let reply: ReturnType<typeof deferred>;
    if (point === 'open') {
      reply = f.holdHide();
      pending = f.modal.open('styles');
    } else {
      await f.modal.open('styles');
      reply = f.holdRestore();
      pending = f.modal.close();
      await Promise.resolve();
      await Promise.resolve();
    }
    f.modal.dispose();
    const count = f.events.length;
    reply.resolve();
    await pending;
    await f.modal.open('fonts');
    await f.modal.close();
    assert(
      !f.modal.active && !f.modal.identity && f.events.length === count,
      'Disposed transition published/focused/sent another command',
    );
  }
  passed(
    'Disposal during hide or restore suppresses all late presentation, focus and follow-up commands',
  );
  {
    const f = fixture();
    const hide = f.holdHide();
    const pending = f.modal.open('styles');
    hide.reject(new Error('hide failed'));
    await pending;
    assert(
      !f.modal.active &&
        !f.modal.identity &&
        f.events.includes('Error: hide failed'),
      'Failed hide kept workspace locked',
    );
    f.modal.dispose();
    const g = fixture();
    await g.modal.open('plugins');
    const restore = g.holdRestore();
    const close = g.modal.close();
    restore.reject(new Error('restore failed'));
    await close;
    assert(
      !g.modal.active &&
        g.events.includes('Error: restore failed') &&
        g.events.at(-1) === 'focus',
      'Failed restore lost error or focus recovery',
    );
    g.modal.dispose();
  }
  passed(
    'Hide/restore failures report errors and release their gates without losing focus recovery',
  );
}
