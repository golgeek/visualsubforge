import assert from 'node:assert/strict';
import test from 'node:test';
import { confirmReviewChange } from './review-change.ts';

const key = { generation: 1, revision: 2, epoch: 3 };
test('cancel returns no permission to send a destructive review command', async () => {
  assert.equal(
    await confirmReviewChange(
      key,
      () => key,
      async () => 'cancel',
      () => assert.fail('Cancel applied a draft'),
    ),
    null,
  );
});
test('consent cannot move to another review epoch during the prompt', async () => {
  let current = key;
  await assert.rejects(
    confirmReviewChange(
      key,
      () => current,
      async () => {
        current = { ...key, epoch: key.epoch + 1 };
        return 'discard';
      },
      () => assert.fail('Discard applied a draft'),
    ),
    /review changed/,
  );
});
test('Save binds consent to the revision accepted for the pending cue draft', async () => {
  let current = key;
  const applied = { ...key, revision: key.revision + 1 };
  const consent = await confirmReviewChange(
    key,
    () => current,
    async () => ({
      save: async () => {
        assert.deepEqual(current, applied);
        return true;
      },
    }),
    async () => {
      current = applied;
      return true;
    },
  );
  assert.deepEqual(consent, { key: applied, discard: true });
});
test('Save rejects an unrelated edit while the prompt is open or the save is pending', async () => {
  for (const duringPrompt of [true, false]) {
    let current = key;
    await assert.rejects(
      confirmReviewChange(
        key,
        () => current,
        async () => {
          if (duringPrompt) current = { ...key, revision: key.revision + 1 };
          return {
            save: async () => {
              assert.equal(duringPrompt, false);
              current = { ...current, revision: current.revision + 1 };
              return true;
            },
          };
        },
        async () => {
          assert.equal(duringPrompt, false);
          current = { ...key, revision: key.revision + 1 };
          return true;
        },
      ),
      /review changed/,
    );
  }
});
