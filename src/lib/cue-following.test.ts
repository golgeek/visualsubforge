import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  activeCueIds,
  changeFollowing,
  followedSelection,
  revealCueRow,
} from './cue-following.ts';
const cues = [
  { id: 7, startMs: 1500, endMs: 3000 },
  { id: 3, startMs: 1000, endMs: 2000 },
  { id: 9, startMs: 3000, endMs: 4000 },
  { id: 11, startMs: 4500, endMs: 4500 },
];
test('half-open intervals preserve document order, adjacent boundaries and gaps', () => {
  for (const [time, expected] of [
    [0, []],
    [0.999999, []],
    [1, [3]],
    [1.5, [7, 3]],
    [1.999999, [7, 3]],
    [2, [7]],
    [3, [9]],
    [4, []],
    [4.5, []],
    [null, []],
    [NaN, []],
    [Infinity, []],
  ] as const)
    assert.deepEqual(activeCueIds(cues, time), expected);
});
test('seeks and loop wraps use current position without a forward-only cursor', () => {
  let selected: number | null = null;
  const selections = [1, 1.6, 2, 3, 4, 1.7, 1, null].map((time) => {
    selected = followedSelection(
      activeCueIds(cues, time),
      selected,
      'following',
      false,
    );
    return selected;
  });
  assert.deepEqual(selections, [3, 3, 7, 9, 9, 7, 3, 3]);
  assert.equal(followedSelection([7, 3], 9, 'following', false), 7);
});
test('off, suspended, dirty and pending operations retain manual selection', () => {
  for (const mode of ['off', 'suspended'] as const)
    assert.equal(followedSelection([7], 3, mode, false), 3);
  assert.equal(followedSelection([7], 3, 'following', true), 3);
  let mode = changeFollowing('following', 'suspend', false);
  assert.equal(mode, 'suspended');
  assert.equal(changeFollowing(mode, 'resume', true), 'suspended');
  // Applying/reverting is not an automatic resume event.
  assert.equal(followedSelection([7], 3, mode, false), 3);
  mode = changeFollowing(mode, 'resume', false);
  assert.equal(followedSelection([7], 3, mode, false), 7);
  assert.equal(changeFollowing('off', 'suspend', true), 'off');
  assert.equal(changeFollowing('off', 'enable', true), 'suspended');
  assert.equal(changeFollowing('suspended', 'disable', true), 'off');
});
test('document replacement and accepted timing edits use fresh cue identities/intervals', () => {
  const replacementCue = { id: 22, startMs: 1000, endMs: 2000 };
  const replacement = [replacementCue];
  assert.equal(
    followedSelection(activeCueIds(replacement, 1.5), 3, 'following', false),
    22,
  );
  assert.deepEqual(
    activeCueIds([{ ...replacementCue, startMs: 1600 }], 1.5),
    [],
  );
  assert.deepEqual(activeCueIds([], 1.5), []);
});
test('virtual list reveals only clipped rows and leaves visible rows in place', () => {
  assert.equal(revealCueRow(2, 64, 96), 64);
  assert.equal(revealCueRow(4, 64, 96), 64);
  assert.equal(revealCueRow(5, 64, 96), 96);
  assert.equal(revealCueRow(1, 64, 96), 32);
  assert.equal(revealCueRow(1000, 0, 96), 31936);
});

test('EOF uses reported position and retains selection when the final cue ends', () => {
  const finalCues = [
    { id: 1, startMs: 45000, endMs: 60000 },
    { id: 2, startMs: 45000, endMs: 70000 },
  ];
  // A retained video frame may precede the reported EOF position. The list
  // does not invent a frame timestamp or special-case the final cue as active.
  assert.deepEqual(activeCueIds(finalCues, 59.96), [1, 2]);
  assert.deepEqual(activeCueIds(finalCues, 60), [2]);
  assert.equal(
    followedSelection(activeCueIds(finalCues, 60), 1, 'following', false),
    2,
  );
  assert.equal(
    followedSelection(
      activeCueIds(finalCues.slice(0, 1), 60),
      1,
      'following',
      false,
    ),
    1,
  );
});
