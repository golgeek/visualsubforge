// Observe only the test app's direct plugin helpers. This harness never signals
// those helpers: cancellation and reaping must be performed by the real owner.
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { setTimeout as delay } from 'node:timers/promises';
import { promisify } from 'node:util';

const run = promisify(execFile);

export async function observePluginChildren(parentPid) {
  assert.ok(Number.isSafeInteger(parentPid) && parentPid > 0);
  if (process.platform !== 'win32') {
    return {
      async sample() {
        const { stdout } = await run('ps', ['-axo', 'pid=,ppid=,command='], {
          timeout: 5000,
        });
        return stdout.split('\n').flatMap((line) => {
          const match = /^\s*(\d+)\s+(\d+)\s+(.*)$/.exec(line);
          return match &&
            Number(match[2]) === parentPid &&
            match[3].includes('--vsf-plugin-worker')
            ? [Number(match[1])]
            : [];
        });
      },
      async close() {},
    };
  }

  // Start PowerShell before the one-second helper invocation. Its cold startup
  // must not consume the interval in which the UI has to cancel the live helper.
  // Every sample() waits for a fresh CIM result, including the post-cancel check.
  const script = `
$ErrorActionPreference = 'Stop'
$watch = [Diagnostics.Stopwatch]::StartNew()
while ($watch.Elapsed.TotalSeconds -lt 45) {
  $children = @(Get-CimInstance Win32_Process -Filter 'ParentProcessId = ${parentPid}' |
    Where-Object { $_.Name -eq 'vsf.exe' -and $_.CommandLine -like '*--vsf-plugin-worker*' } |
    ForEach-Object { [int]$_.ProcessId })
  [Console]::WriteLine((ConvertTo-Json -InputObject $children -Compress))
  Start-Sleep -Milliseconds 50
}`;
  const child = spawn(
    'powershell.exe',
    ['-NoProfile', '-NonInteractive', '-Command', script],
    { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true },
  );
  let stderr = '',
    failure,
    closing = false;
  const waiters = new Set();
  const fail = (error) => {
    failure = error;
    for (const waiter of [...waiters]) waiter.reject(error);
  };
  child.stderr.on('data', (bytes) => {
    stderr = (stderr + bytes.toString()).slice(-8192);
  });
  child.on('error', fail);
  const exited = new Promise((resolve) => child.once('close', resolve));
  child.once('exit', (code, signal) => {
    if (!closing)
      fail(new Error(`Plugin observer exited (${code}/${signal}): ${stderr}`));
  });
  const lines = createInterface({ input: child.stdout });
  lines.on('line', (line) => {
    try {
      const pids = JSON.parse(line);
      assert.ok(Array.isArray(pids));
      assert.ok(pids.every((pid) => Number.isSafeInteger(pid) && pid > 0));
      for (const waiter of [...waiters]) waiter.resolve(pids);
    } catch (error) {
      fail(error);
    }
  });
  const observer = {
    sample() {
      if (failure) return Promise.reject(failure);
      return new Promise((resolve, reject) => {
        const finish = (callback, value) => {
          clearTimeout(timer);
          waiters.delete(waiter);
          callback(value);
        };
        const waiter = {
          resolve: (value) => finish(resolve, value),
          reject: (error) => finish(reject, error),
        };
        const timer = setTimeout(
          () => waiter.reject(new Error(`Plugin observer stalled: ${stderr}`)),
          10_000,
        );
        waiters.add(waiter);
      });
    },
    async close() {
      closing = true;
      fail(new Error('Plugin observer closed'));
      if (child.exitCode === null && child.signalCode === null) child.kill();
      await Promise.race([
        exited,
        delay(5000, undefined, { ref: false }).then(() => {
          throw new Error('Plugin observer did not exit');
        }),
      ]);
      lines.close();
    },
  };
  try {
    await observer.sample();
    return observer;
  } catch (error) {
    await observer.close();
    throw error;
  }
}
