import type { ReviewReply } from '../client';

type Key = ReviewReply['key'];

function matches(a: Key, b: Key) {
  return (
    a.generation === b.generation &&
    a.revision === b.revision &&
    a.epoch === b.epoch
  );
}

/** Consent belongs to the review the user saw when the decision opened. */
export async function confirmReviewChange(
  key: Key,
  current: () => Key,
  needsConsent: boolean,
  protect: () => Promise<boolean>,
) {
  if (!matches(key, current()))
    throw new Error('The review changed; refresh first.');
  if (needsConsent && !(await protect())) return null;
  if (!matches(key, current()))
    throw new Error('The review changed; refresh first.');
  return { key, discard: needsConsent };
}
