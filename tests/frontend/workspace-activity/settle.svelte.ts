import { flushSync } from 'svelte';
import { settled } from '../../../src/lib/ui/settled.svelte';
import { WorkspaceActivity } from '../../../src/lib/workspace/workspace-activity.svelte';

export function observe() {
  let pending = $state(false);
  let view!: ReturnType<typeof settled>;
  const dispose = $effect.root(() => {
    view = settled(() => pending);
  });
  const activity = new WorkspaceActivity();
  return {
    activity,
    pending(value: boolean) {
      flushSync(() => (pending = value));
    },
    updating: () => view.showUpdating,
    dispose,
  };
}
