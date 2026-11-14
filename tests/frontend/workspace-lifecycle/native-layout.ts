import { NativeVideoLayout } from '../../../src/lib/media/native-video-layout';
import { emptyStatus } from '../../../src/lib/client';
export async function nativeLayoutCases(passed: (message: string) => void) {
  const assert = (condition: unknown, message: string) => {
    if (!condition) throw new Error(message);
  };
  const host = document.createElement('div');
  host.style.cssText = 'position:fixed;left:0;top:0;width:100px;height:100px';
  document.body.append(host);
  let calls = 0;
  let reject!: (error: Error) => void;
  const messages = { error: '' };
  const owner = new NativeVideoLayout(messages, {
    status: { ...emptyStatus, available: true, loaded: true },
    layout: () => {
      ++calls;
      return new Promise<void>((_, fail) => {
        reject = fail;
      });
    },
  });
  try {
    owner.mount(host);
    window.dispatchEvent(new Event('resize'));
    window.dispatchEvent(new Event('scroll'));
    assert(calls === 1, 'Layout requests must coalesce while one is pending');
    owner.dispose();
    reject(new Error('late native layout failure'));
    await Promise.resolve();
    await Promise.resolve();
    window.dispatchEvent(new Event('resize'));
    window.dispatchEvent(new Event('scroll'));
    await new Promise((resolve) => setTimeout(resolve, 130));
    assert(
      calls === 1 && messages.error === '',
      'Disposal must remove listeners/timer and suppress late failure',
    );
    passed(
      'Native layout coalesces requests and stops observation and late publication on disposal',
    );
  } finally {
    owner.dispose();
    host.remove();
  }
  const moving = document.createElement('div');
  moving.style.cssText =
    'position:fixed;left:20px;top:30px;width:100px;height:100px';
  document.body.append(moving);
  let release!: () => void;
  const widths: number[] = [];
  const visibility: boolean[] = [];
  const draining = new NativeVideoLayout(messages, {
    status: { ...emptyStatus, available: true, loaded: true },
    layout: async (bounds) => {
      widths.push(bounds.width);
      visibility.push(bounds.visible);
      if (widths.length === 1)
        await new Promise<void>((resolve) => {
          release = resolve;
        });
    },
  });
  try {
    draining.mount(moving);
    window.dispatchEvent(new Event('resize'));
    moving.style.width = '200px';
    let acknowledged = false;
    const flush = draining.flush().then(() => {
      acknowledged = true;
    });
    await Promise.resolve();
    assert(
      !acknowledged && widths.length === 1,
      'Flush waits for the old in-flight native bounds',
    );
    release();
    await flush;
    assert(
      widths.join(',') === '100,200',
      'Flush acknowledges current rendered bounds after draining the old position',
    );
    passed(
      'Native layout flush drains old bounds and acknowledges current geometry before returning',
    );
    assert(visibility.at(-1) === true, 'Positive in-window bounds are visible');
    moving.style.width = '0px';
    await draining.flush();
    assert(
      visibility.at(-1) === false,
      'Zero width hides a surface inside the viewport',
    );
    moving.style.width = '200px';
    moving.style.height = '0px';
    await draining.flush();
    assert(
      visibility.at(-1) === false,
      'Zero height hides a surface inside the viewport',
    );
    moving.style.height = '100px';
    await draining.flush();
    assert(
      visibility.at(-1) === true,
      'Restored positive bounds reveal the surface',
    );
    passed(
      'Native layout hides either zero dimension and reveals restored positive bounds',
    );
  } finally {
    draining.dispose();
    moving.remove();
  }
}
