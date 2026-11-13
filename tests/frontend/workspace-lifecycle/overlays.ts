import { HideVideo } from '../../../src/lib/hide-video.svelte';

export async function overlayCases(passed: (message: string) => void) {
  const assert = (condition: unknown, message: string) => {
    if (!condition) throw new Error(message);
  };
  const events: string[] = [];
  let failHide = false;
  let release!: () => void;
  let render: Promise<void> | null = null;
  const video = new HideVideo({
    occlude: async (hidden) => {
      events.push(`occlude:${hidden}`);
      if (hidden && failHide) throw new Error('hide refused');
    },
    remainingOcclusion: () => true,
    afterRender: async () => {
      events.push('render');
      await render;
    },
  });
  let content = 'first';
  await video.open(() => {
    content = 'first';
  });
  await video.open(
    () => {
      content = 'replacement';
    },
    { replace: true },
  );
  assert(
    content === 'replacement' && video.visible,
    'Menu replacement must be presented',
  );
  failHide = true;
  let failed = false;
  try {
    await video.open(
      () => {
        content = 'failed';
      },
      { replace: true },
    );
  } catch {
    failed = true;
  }
  assert(
    failed && content === 'replacement' && video.visible,
    'Failed replacement must retain the open menu',
  );
  passed(
    'Shared video lifecycle preserves menu replacement and failed replacement content',
  );
  failHide = false;
  events.length = 0;
  await video.close({
    hide: () => {
      content = '';
    },
    failed: () => {
      throw new Error('Unexpected restore failure');
    },
  });
  assert(
    content === '' &&
      events.join(',') === 'render,occlude:true' &&
      !video.active,
    'Menu removal must render before restoring remaining occlusion',
  );
  passed(
    'Shared video lifecycle renders removal and restores remaining occlusion',
  );

  await video.open(() => {});
  render = new Promise<void>((resolve) => {
    release = resolve;
  });
  events.length = 0;
  const closing = video.close({
    hide: () => {},
    failed: () => {
      events.push('error');
    },
    focus: () => {
      events.push('focus');
    },
  });
  await Promise.resolve();
  await Promise.resolve();
  video.dispose();
  release();
  await closing;
  assert(
    !events.some(
      (event) =>
        event.startsWith('occlude:') || event === 'focus' || event === 'error',
    ),
    'Disposal during rendering must suppress restoration and focus',
  );
  passed(
    'Shared video lifecycle suppresses close continuations after disposal',
  );
}
