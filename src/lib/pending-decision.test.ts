import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PendingDecision } from './pending-decision.ts';

test('disposal settles an open decision and cannot authorize a later answer', async () => {
  const decision = new PendingDecision<'cancel' | 'save'>('cancel');
  const result = decision.wait();
  assert.equal(decision.pending, true);
  decision.dispose();
  decision.answer('save');
  assert.equal(await result, 'cancel');
  assert.equal(decision.pending, false);
  assert.equal(await decision.wait(), 'cancel');
});

test('a decision rejects overlapping prompts but can be used again after an answer', async () => {
  const decision = new PendingDecision<'cancel' | 'retry' | 'quit'>('cancel');
  const first = decision.wait();
  assert.throws(() => decision.wait(), /already pending/);
  decision.answer('retry');
  decision.answer('quit');
  assert.equal(await first, 'retry');
  const next = decision.wait();
  decision.answer('quit');
  assert.equal(await next, 'quit');
  decision.dispose();
});
