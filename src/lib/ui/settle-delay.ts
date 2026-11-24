// Cancelling the hold never cancels the operation whose progress it presents.
export const SETTLE_MS = 150;
export function afterSettle(show: () => void) {
  const timer = setTimeout(show, SETTLE_MS);
  return () => clearTimeout(timer);
}
