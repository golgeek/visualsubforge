import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatDateTime, setDateRegion } from './date-format.ts';

test('English words follow the system region, with local hours and no seconds', () => {
  const ms = new Date(2027, 3, 20, 15, 22, 51).getTime();
  for (const [region, expected] of [
    ['US', 'Apr 20, 2027, 3:22 PM'],
    ['GB', '20 Apr 2027, 15:22'],
    ['FR', '20 Apr 2027, 15:22'],
    [null, '20 Apr 2027, 15:22'],
    ['ZZ', '20 Apr 2027, 15:22'],
  ] as const) {
    setDateRegion(region);
    const text = formatDateTime(ms).replace(/[\u202f\u00a0]/gu, ' ');
    assert.equal(text, expected);
    assert.doesNotMatch(text, /avr|\d+[./-]\d+[./-]\d+/iu);
  }
  setDateRegion('MV');
  assert.match(formatDateTime(ms), /Apr/);
});
