import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { LiveLoop } from './live-loop.ts';
function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
test('live loop coalesces previews and drains restoration before clear', async () => {
  const wait = deferred();
  const sent: [number, number][] = [];
  const loop = new LiveLoop(
    async (_, range) => {
      sent.push(range);
      if (sent.length === 1) await wait.promise;
    },
    (error) => assert.fail(String(error)),
  );
  loop.update(1, [1, 2]);
  assert.equal(sent.length, 0);
  loop.activate(1);
  loop.update(1, [1, 2]);
  loop.update(1, [1, 3]);
  loop.update(1, [1, 4]);
  assert.deepEqual(sent, [[1, 2]]);
  wait.resolve();
  await loop.flush();
  assert.deepEqual(sent, [
    [1, 2],
    [1, 4],
  ]);
  loop.update(1, [1, 2]);
  await loop.flush();
  assert.deepEqual(sent.at(-1), [1, 2]);
  await loop.stop();
  loop.update(1, [2, 3]);
  loop.update(2, [3, 4]);
  assert.equal(sent.length, 3);
});
test('stop drops pending previews and waits for an in-flight update', async () => {
  const wait = deferred();
  let calls = 0;
  let stopped = false;
  const loop = new LiveLoop(
    async () => {
      calls++;
      await wait.promise;
    },
    (error) => assert.fail(String(error)),
  );
  loop.activate(3);
  loop.update(3, [1, 2]);
  loop.update(3, [1, 4]);
  const done = loop.stop().then(() => {
    stopped = true;
  });
  await Promise.resolve();
  assert.equal(stopped, false);
  wait.resolve();
  await done;
  assert.equal(calls, 1);
});
test('failed preview reports error and permits a later restoration', async () => {
  let failures = 0;
  let calls = 0;
  const loop = new LiveLoop(
    async () => {
      if (++calls === 1) throw new Error('failed');
    },
    () => {
      failures++;
    },
  );
  loop.activate(1);
  loop.update(1, [1, 2]);
  await loop.flush();
  loop.update(1, [2, 3]);
  await loop.flush();
  assert.equal(failures, 1);
  assert.equal(calls, 2);
});
