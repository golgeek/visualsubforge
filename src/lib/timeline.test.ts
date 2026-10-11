import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  formatTime,
  parseTime,
  orderedRange,
  timeToX,
  xToTime,
  viewport,
} from './timeline.ts';
test('viewport mapping preserves nonzero origin, clips edges and orders reverse drags', () => {
  for (const time of [12, 12.001, 17.6, 32])
    assert.ok(
      Math.abs(xToTime(timeToX(time, 12, 20, 1234), 12, 20, 1234) - time) <
        1e-10,
    );
  assert.equal(xToTime(-20, 12, 20, 1234), 12);
  assert.equal(xToTime(1500, 12, 20, 1234), 32);
  assert.deepEqual(orderedRange(8, -2, 10), [0, 8]);
  assert.deepEqual(viewport(95, 20, 100), { start: 80, span: 20 });
  assert.deepEqual(viewport(0, 20, 0), { start: 0, span: 0.1 });
});
test('millisecond time editing round trips and rejects malformed values', () => {
  for (const ms of [0, 1, 999, 1000, 59999, 3600000, 35999999999])
    assert.equal(parseTime(formatTime(ms)), ms);
  for (const time of [
    '-1:00:00,000',
    '00:60:00,000',
    '00:00:60,000',
    '00:00:01,12',
    'text',
  ])
    assert.throws(() => parseTime(time));
});
