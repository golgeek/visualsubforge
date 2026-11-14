import { NativeVideoLayout } from '../../../src/lib/native-video-layout';
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
}
