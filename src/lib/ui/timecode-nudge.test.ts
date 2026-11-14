import test from 'node:test';
import assert from 'node:assert/strict';
import { nudgeTimecode } from './timecode-nudge.ts';
test('nominal frame nudge quantizes at document precision and clamps zero', () => {
  assert.equal(nudgeTimecode('00:00:01,000', 1, 24, 1), '00:00:01,042');
  assert.equal(
    nudgeTimecode('00:00:01,000', 1, 24000 / 1001, 10),
    '00:00:01,040',
  );
  assert.equal(nudgeTimecode('00:00:00,010', -1, 25, 1), '00:00:00,000');
  assert.throws(() => nudgeTimecode('invalid', 1, 25, 1), /HH:MM:SS/);
  for (const rate of [0, NaN, Infinity, 1001])
    assert.throws(
      () => nudgeTimecode('00:00:01,000', 1, rate, 1),
      /unavailable/,
    );
});
