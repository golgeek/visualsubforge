import { test } from 'node:test';
import { strict as assert } from 'node:assert';
import { defaultTiming, snapBoundary } from './magnetic-timing.ts';
const settings = { ...defaultTiming, enabled: true };
const cues = [
  { id: 1, startMs: 1000, endMs: 2000, text: 'Previous' },
  { id: 2, startMs: 3000, endMs: 4000, text: 'Edited' },
  { id: 3, startMs: 5000, endMs: 6000, text: 'Next' },
];
test('Cue edges land at the configured gap and ignore their own accepted edges', () => {
  assert.deepEqual(
    snapBoundary(2090, 'start', 4000, 2, cues, [], settings, 30, 10000),
    { milliseconds: 2080, target: 'cue gap' },
  );
  assert.equal(
    snapBoundary(4900, 'end', 3000, 2, cues, [], settings, 30, 10000)
      .milliseconds,
    4920,
  );
  assert.equal(
    snapBoundary(3080, 'end', 3000, 2, cues, [], settings, 30, 10000).target,
    null,
  );
  assert.equal(
    snapBoundary(
      2010,
      'start',
      4000,
      2,
      cues,
      [],
      { ...settings, cueGapMs: 0 },
      30,
      10000,
    ).milliseconds,
    2000,
  );
});
test('Scene start/end offsets are directional and rounded once to milliseconds', () => {
  assert.equal(
    snapBoundary(2120, 'start', 4000, null, [], [2.0004], settings, 30, 10000)
      .milliseconds,
    2130,
  );
  assert.equal(
    snapBoundary(1880, 'end', 1000, null, [], [2], settings, 30, 10000)
      .milliseconds,
    1870,
  );
});
test('Bypass, disabled magnets, exact tolerance and media/duration bounds', () => {
  for (const [policy, bypass] of [
    [defaultTiming, false],
    [settings, true],
  ] as const)
    assert.equal(
      snapBoundary(2090, 'start', 4000, 2, cues, [], policy, 30, 10000, bypass)
        .milliseconds,
      2090,
    );
  assert.equal(
    snapBoundary(2090, 'start', 4000, 2, cues, [], settings, 9, 10000).target,
    null,
  );
  assert.equal(
    snapBoundary(2090, 'start', 4000, 2, cues, [], settings, 10, 10000)
      .milliseconds,
    2080,
  );
  assert.equal(
    snapBoundary(2090, 'start', 2050, 2, cues, [], settings, 30, 10000).target,
    null,
  );
  assert.equal(
    snapBoundary(6100, 'start', 7000, 2, cues, [], settings, 30, 6050).target,
    null,
  );
});
test('Conflicting cues and overlapping scene buffers never offer an invalid landing', () => {
  assert.equal(
    snapBoundary(
      2090,
      'start',
      4000,
      2,
      [...cues, { id: 4, startMs: 2050, endMs: 2500, text: '' }],
      [],
      settings,
      30,
      10000,
    ).target,
    null,
  );
  assert.equal(
    snapBoundary(2130, 'start', 4000, null, [], [2, 2.1], settings, 30, 10000)
      .target,
    null,
  );
  assert.equal(
    snapBoundary(2090, 'start', 4000, 2, cues, [2.1], settings, 30, 10000)
      .target,
    null,
  );
});
test('Ties favor cue gaps, then earlier timestamps regardless of input order', () => {
  const noBefore = { ...settings, sceneBeforeMs: 0, sceneAfterMs: 0 };
  assert.equal(
    snapBoundary(2090, 'start', 4000, 2, cues, [2.1], noBefore, 30, 10000)
      .target,
    'cue gap',
  );
  assert.equal(
    snapBoundary(
      2000,
      'start',
      4000,
      null,
      [],
      [2.01, 1.99],
      noBefore,
      30,
      10000,
    ).milliseconds,
    1990,
  );
});
