import { defaultPreferences } from '../../../src/lib/client';
import { mount, tick, unmount } from 'svelte';
import { isTauri } from '@tauri-apps/api/core';
import App from '../../../src/App.svelte';
import { emptyDocument, type RecoveryStatus } from '../../../src/lib/client';

// Exercise the actual App and typed client in a browser. Never replace real IPC
// in a native application; only the three commands used here are mocked.
if (isTauri()) throw new Error('Run this fixture in an ordinary browser.');
const target = document.getElementById('test');
const result = document.getElementById('result');
const checks = document.getElementById('checks');
if (!target || !result || !checks) throw new Error('Missing fixture elements');
const host = target;
const heading = result;
const output = checks;

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
const stale: RecoveryStatus = {
  entries: [
    {
      id: 'session-old',
      sourcePath: '/disposable/old.srt',
      capturedMs: null,
      revision: 2,
      error: null,
    },
  ],
  error: null,
};
const fresh: RecoveryStatus = {
  entries: [
    {
      ...stale.entries[0]!,
      id: 'session-new',
      sourcePath: '/disposable/new.srt',
    },
  ],
  error: null,
};
function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}
async function settle() {
  await tick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  await tick();
}
async function until(condition: () => boolean, message: string) {
  const deadline = performance.now() + 8000;
  while (!condition()) {
    if (performance.now() > deadline) throw new Error(message);
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  await settle();
}
function button(label: string): HTMLButtonElement {
  const found = [...host.querySelectorAll('button')].find(
    (item) => item.textContent?.trim() === label,
  );
  if (!found) throw new Error(`Missing button: ${label}`);
  return found;
}
function hasEntry(name: string): boolean {
  return [...host.querySelectorAll('.recovery-row')].some((row) =>
    row.textContent?.includes(name),
  );
}
function passed(message: string) {
  const item = document.createElement('li');
  item.textContent = `PASS: ${message}`;
  output.append(item);
}
async function runCase(mode: 'stale list' | 'stale error' | 'failed discard') {
  const poll = deferred<RecoveryStatus>();
  const discard = deferred<void>();
  let polls = 0;
  let discardPending = false;
  let pollStartedDuringDiscard = false;
  Object.defineProperty(window, '__TAURI_INTERNALS__', {
    configurable: true,
    value: {
      invoke(command: string) {
        if (command === 'preferences_get')
          return structuredClone(defaultPreferences);
        switch (command) {
          case 'document_action':
            return Promise.resolve({ ...emptyDocument });
          case 'recovery_status':
            polls++;
            if (polls === 1) return Promise.resolve(structuredClone(stale));
            if (polls === 2) {
              pollStartedDuringDiscard = discardPending;
              return poll.promise;
            }
            return Promise.resolve(structuredClone(fresh));
          case 'recovery_discard':
            discardPending = true;
            return discard.promise;
          default:
            throw new Error(`Unexpected IPC command: ${command}`);
        }
      },
    },
  });
  const app = mount(App, { target: host });
  try {
    await until(
      () => hasEntry('old.srt') && !button('Recover').disabled,
      'Initial recovery list did not become ready',
    );
    button('Discard…').click();
    await settle();
    button('Confirm discard').click();
    // Keep real production polling intervals: the second read must begin
    // while the asynchronous mutation is pending, not before its invalidation.
    await until(() => polls === 2, 'No poll during pending discard');
    assert(pollStartedDuringDiscard, 'Poll did not overlap the discard');
    if (mode === 'failed discard') {
      discard.reject(new Error('Disposable discard failure'));
      await settle();
      assert(hasEntry('old.srt'), 'Failed discard removed the recovery entry');
      assert(
        !!host.textContent?.includes('Disposable discard failure'),
        'Failed discard error was hidden',
      );
      poll.resolve(structuredClone(stale));
      await settle();
      assert(hasEntry('old.srt'), 'Pending read lost the retained entry');
      passed('failed discard retains its entry and reports the error');
    } else {
      discard.resolve();
      await settle();
      assert(
        !hasEntry('old.srt'),
        'Successful discard did not remove the entry',
      );
      if (mode === 'stale list') poll.resolve(structuredClone(stale));
      else poll.reject(new Error('Obsolete recovery-list failure'));
      await settle();
      assert(!hasEntry('old.srt'), 'Late poll resurrected the discarded entry');
      assert(
        !host.textContent?.includes('Obsolete recovery-list failure'),
        'Late poll surfaced an obsolete error after discard',
      );
      passed(`${mode} started during discard is ignored after success`);
      if (mode === 'stale list') {
        await until(() => hasEntry('new.srt'), 'Fresh polling did not resume');
        passed('a subsequent current poll still discovers a new entry');
      }
    }
  } finally {
    await unmount(app);
    Reflect.deleteProperty(window, '__TAURI_INTERNALS__');
  }
}
async function run() {
  try {
    for (const mode of ['stale list', 'stale error', 'failed discard'] as const)
      await runCase(mode);
    heading.textContent = 'PASS: all 4 recovery response ordering checks';
  } catch (error) {
    heading.textContent = `FAIL: ${String(error)}`;
  }
}
void run();
