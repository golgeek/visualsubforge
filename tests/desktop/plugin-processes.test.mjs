import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { observePluginChildren } from './plugin-processes.mjs';

test(
  'observes the owned real helper and a fresh empty snapshot after exit',
  {
    timeout: 30_000,
  },
  async () => {
    const observer = await observePluginChildren(process.pid);
    let child;
    let exited;
    try {
      assert.deepEqual(await observer.sample(), []);
      const runner = resolve(
        process.env.VSF_PLUGIN_OBSERVER_RUNNER ??
          `target/debug/vsf${process.platform === 'win32' ? '.exe' : ''}`,
      );
      // The production helper waits for a request on stdin. No synthetic process
      // name, mocked process table, script execution or changed execution limit.
      child = spawn(runner, ['--vsf-plugin-worker'], {
        stdio: ['pipe', 'ignore', 'ignore'],
        windowsHide: true,
      });
      exited = once(child, 'close');
      void exited.catch(() => {});
      await once(child, 'spawn');
      const deadline = Date.now() + 5000;
      let observed = [];
      while (Date.now() < deadline && !observed.length)
        observed = await observer.sample();
      assert.deepEqual(observed, [child.pid]);
      child.stdin.end();
      await Promise.race([
        exited,
        delay(5000, undefined, { ref: false }).then(() => {
          throw new Error('Helper did not exit after request EOF');
        }),
      ]);
      assert.deepEqual(await observer.sample(), []);
    } finally {
      if (child && child.exitCode === null && child.signalCode === null) {
        child.kill();
        await exited;
      }
      await observer.close();
    }
  },
);
