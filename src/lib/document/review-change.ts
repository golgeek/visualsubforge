import type { ReviewReply } from '../client';

import type { UnsavedDecision } from './unsaved-changes';

type Key = ReviewReply['key'];

function requireKey(expected: Key, actual: Key) {
  if (
    expected.generation !== actual.generation ||
    expected.revision !== actual.revision ||
    expected.epoch !== actual.epoch
  )
    throw new Error('The review changed; refresh first.');
}

/** Only the user's own accepted draft may advance consent to a new revision. */
export async function confirmReviewChange(
  key: Key,
  current: () => Key,
  decide: (() => Promise<UnsavedDecision>) | null,
  applyDraft: () => Promise<boolean>,
) {
  requireKey(key, current());
  if (!decide) return { key, discard: false };
  const decision = await decide();
  if (decision === 'cancel') return null;
  requireKey(key, current());
  if (typeof decision !== 'object')
    return { key, discard: decision === 'discard' };

  // Pin the accepted revision before file choosers, IO or project refresh.
  if (!(await applyDraft())) return null;
  const applied = current();
  requireKey({ ...key, revision: applied.revision }, applied);
  if (!(await decision.save())) return null;
  requireKey(applied, current());
  return { key: applied, discard: true };
}
