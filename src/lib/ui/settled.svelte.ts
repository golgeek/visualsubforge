import { afterSettle } from './settle-delay';

export function settled(pending: () => boolean) {
  let elapsed = $state(false);
  $effect(() => {
    if (pending()) return afterSettle(() => (elapsed = true));
    elapsed = false;
  });
  return {
    get showUpdating() {
      return pending() && elapsed;
    },
  };
}
