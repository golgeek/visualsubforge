import assert from 'node:assert/strict';
import { test } from 'node:test';
import { waitForDesktopDriver } from './driver-readiness.mjs';

test('only a responding first Windows CI launch gets a longer WebView2 wait', async (t) => {
  t.mock.timers.enable({ apis: ['Date'], now: 0 });
  async function launch(listensAt, readyAt, coldWindowsStart) {
    const start = Date.now();
    const elapsed = () => Date.now() - start;
    const readReady = () =>
      elapsed() < listensAt ? undefined : elapsed() >= readyAt;
    async function until(label, probe, timeout) {
      const deadline = Date.now() + timeout;
      while (Date.now() < deadline) {
        if (await probe()) return true;
        t.mock.timers.tick(100);
      }
      throw new Error(label);
    }
    await waitForDesktopDriver(until, readReady, coldWindowsStart);
    return elapsed();
  }

  assert.equal(await launch(140, 38_621, true), 38_700);
  await assert.rejects(launch(140, 38_621, false), /WebDriver server/);
  await assert.rejects(launch(31_000, 32_000, true), /WebDriver listener/);
  await assert.rejects(launch(29_000, 61_000, true), /WebView2 window/);
});
