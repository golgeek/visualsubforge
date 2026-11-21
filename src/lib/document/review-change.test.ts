import assert from 'node:assert/strict';
import test from 'node:test';
import { confirmReviewChange } from './review-change.ts';

const key = { generation: 1, revision: 2, epoch: 3 };
test('cancel returns no permission to send a destructive review command', async () => {
  assert.equal(
    await confirmReviewChange(
      key,
      () => key,
      true,
      async () => false,
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
      true,
      async () => {
        current = { ...key, epoch: key.epoch + 1 };
        return true;
      },
    ),
    /review changed/,
  );
});
