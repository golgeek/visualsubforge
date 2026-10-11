import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  beginHeldTiming,
  advanceHeldTiming,
  finishHeldTiming,
  resolveTimingRelease,
} from './hold-timing.ts';
const document = { generation: 4, revision: 9 };
const clock = {
  session: 3,
  loaded: true,
  paused: false,
  seeking: false,
  positionSeconds: 2,
};

test('Hold uses source time and completes one positive millisecond interval', () => {
  const held = beginHeldTiming(document, clock)!;
  assert.deepEqual(
    finishHeldTiming(held, document, { ...clock, positionSeconds: 3.25 }),
    [2, 3.25],
  );
  assert.equal(finishHeldTiming(held, document, clock), null);
  assert.equal(
    finishHeldTiming(held, document, { ...clock, positionSeconds: 2.0001 }),
    null,
  );
});
test('Unknown clocks, pause, seeking, replacement and history changes cancel', () => {
  const held = beginHeldTiming(document, clock)!;
  for (const changed of [
    { paused: true },
    { loaded: false },
    { seeking: true },
    { positionSeconds: null },
    { positionSeconds: NaN },
  ]) {
    assert.equal(beginHeldTiming(document, { ...clock, ...changed }), null);
    assert.equal(
      advanceHeldTiming(held, document, { ...clock, ...changed }),
      null,
    );
  }
  assert.equal(
    advanceHeldTiming(held, document, { ...clock, session: 4 }),
    null,
  );
  assert.equal(
    advanceHeldTiming(held, { ...document, revision: 10 }, clock),
    null,
  );
  assert.equal(
    advanceHeldTiming(held, { ...document, generation: 5 }, clock),
    null,
  );
});
test('Backward movement including a loop wrap invalidates the entire hold', () => {
  const held = advanceHeldTiming(beginHeldTiming(document, clock)!, document, {
    ...clock,
    positionSeconds: 3,
  })!;
  assert.equal(
    advanceHeldTiming(held, document, { ...clock, positionSeconds: 2.9 }),
    null,
  );
});

test('Short Space pauses even between clock samples; threshold uses key duration', () => {
  const held = beginHeldTiming(document, clock)!;
  assert.deepEqual(resolveTimingRelease(held, document, clock, 100), {
    kind: 'pause',
  });
  assert.deepEqual(
    resolveTimingRelease(held, document, { ...clock, positionSeconds: 9 }, 399),
    { kind: 'pause' },
  );
  assert.deepEqual(
    resolveTimingRelease(
      held,
      document,
      { ...clock, positionSeconds: 2.1 },
      400,
    ),
    { kind: 'create', range: [2, 2.1] },
  );
  assert.equal(resolveTimingRelease(held, document, clock, 500), null);
});
test('Cancelled or stale Space cannot pause or create on release', () => {
  const held = beginHeldTiming(document, clock)!;
  for (const duration of [100, 500]) {
    assert.equal(
      resolveTimingRelease(
        held,
        document,
        { ...clock, paused: true },
        duration,
      ),
      null,
    );
    assert.equal(
      resolveTimingRelease(
        held,
        { ...document, revision: 10 },
        clock,
        duration,
      ),
      null,
    );
    assert.equal(
      resolveTimingRelease(
        held,
        document,
        { ...clock, positionSeconds: 1 },
        duration,
      ),
      null,
    );
  }
  for (const duration of [-1, NaN, Infinity])
    assert.equal(resolveTimingRelease(held, document, clock, duration), null);
});
