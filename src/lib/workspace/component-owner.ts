import { onDestroy } from 'svelte';
/** Register cleanup beside construction; call only during component initialization. */
export function disposeWithComponent<T extends { dispose(): void }>(
  owner: T,
): T {
  onDestroy(() => owner.dispose());
  return owner;
}
