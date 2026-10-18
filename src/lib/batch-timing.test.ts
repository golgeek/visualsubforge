import { test } from 'node:test';
import assert from 'node:assert/strict';
import { sameBatchRequest } from './batch-timing.ts';
import type { BatchTimingRequest } from './client';
test('Scope identity is semantic across derived recomputations and guards every timing input', () => {
  const a: BatchTimingRequest = {
    scope: { kind: 'selected', cueIds: [1, 3] },
    change: { kind: 'shift', earlier: false, amountMs: 100 },
  };
  assert(sameBatchRequest(a, structuredClone(a)));
  assert(
    !sameBatchRequest(a, { ...a, scope: { kind: 'selected', cueIds: [1, 2] } }),
  );
  assert(
    !sameBatchRequest(a, {
      ...a,
      change: { kind: 'shift', earlier: true, amountMs: 100 },
    }),
  );
  assert(
    !sameBatchRequest(a, {
      ...a,
      change: { kind: 'shift', earlier: false, amountMs: 200 },
    }),
  );
  assert(
    !sameBatchRequest(a, { ...a, change: { kind: 'align', playheadMs: 100 } }),
  );
  for (const scope of [
    { kind: 'all' },
    { kind: 'following', cueId: 1 },
    { kind: 'atOrAfter', positionMs: 50 },
  ] as const) {
    const request: BatchTimingRequest = { ...a, scope };
    assert(sameBatchRequest(request, structuredClone(request)));
    assert(!sameBatchRequest(request, a));
  }
  assert(!sameBatchRequest(a, null));
});
