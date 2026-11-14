import { Tooltips } from '../../../src/lib/ui/tooltips.svelte';

export async function tooltipCases(passed: (message: string) => void) {
  const assert = (value: unknown, message: string) => {
    if (!value) throw new Error(message);
  };
  const events: string[] = [];
  let pendingHide: Promise<void> | null = null;
  let remaining = false;
  let available = true;
  const anchor = document.createElement('button');
  document.body.append(anchor);
  const owner = new Tooltips(
    {
      occlude: async (value) => {
        events.push(`hide:${value}`);
        if (value) await pendingHide;
      },
      remainingOcclusion: () => remaining,
      afterRender: async () => {
        events.push('render');
      },
    },
    (error) => {
      throw error;
    },
    () => available,
  );
  async function until(predicate: () => boolean) {
    const end = performance.now() + 3000;
    while (!predicate()) {
      assert(performance.now() < end, 'Tooltip lifecycle timed out');
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
  }
  try {
    available = false;
    owner.show('splitCue', anchor);
    await new Promise((resolve) => setTimeout(resolve, 400));
    assert(!events.length, 'A modal or menu must block new tooltips');
    available = true;
    owner.show('splitCue', anchor);
    await owner.hide();
    await new Promise((resolve) => setTimeout(resolve, 400));
    assert(
      !events.length && !owner.content,
      'Leaving during delay must not hide video or present a tooltip',
    );
    let release!: () => void;
    pendingHide = new Promise((resolve) => {
      release = resolve;
    });
    owner.show('splitCue', anchor);
    await until(() => events.includes('hide:true'));
    assert(!owner.content, 'Tooltip must wait for native hide');
    const closing = owner.hide();
    release();
    await closing;
    assert(
      !owner.content && events.join(',') === 'hide:true,render,hide:false',
      'Leaving during hide must retire presentation and restore in order',
    );
    passed(
      'Tooltip delays cancel cleanly and pending native hide cannot publish after pointer leave',
    );
    pendingHide = null;
    events.length = 0;
    remaining = true;
    owner.show('splitCue', anchor);
    await until(() => !!owner.content);
    await owner.hide();
    assert(
      events.join(',') === 'hide:true,render,hide:true',
      'Tooltip close must preserve another overlay',
    );
    events.length = 0;
    pendingHide = new Promise((resolve) => {
      release = resolve;
    });
    owner.show('splitCue', anchor);
    await until(() => events.includes('hide:true'));
    owner.dispose();
    release();
    await Promise.resolve();
    await Promise.resolve();
    assert(
      !owner.content && events.join(',') === 'hide:true',
      'Disposal must suppress late content and leave native cleanup to media teardown',
    );
    passed(
      'Tooltip restoration preserves other overlays and disposal rejects late native replies',
    );
  } finally {
    owner.dispose();
    anchor.remove();
  }
}
