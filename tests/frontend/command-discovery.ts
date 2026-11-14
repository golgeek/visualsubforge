import { tick } from 'svelte';
import type { ActionId } from '../../src/lib/workspace/action-registry';

/** Exercise the visible Commands menu; feature assertions stay in their harness. */
export async function openCommands(host: Element) {
  const launcher = [...host.querySelectorAll<HTMLButtonElement>('button')].find(
    (button) => button.textContent?.trim() === 'Commands',
  );
  if (!launcher || launcher.disabled)
    throw new Error('Commands launcher unavailable');
  launcher.focus();
  launcher.click();
  const end = performance.now() + 5000;
  while (!host.querySelector('dialog[open] [data-action]')) {
    if (performance.now() > end) throw new Error('Commands menu did not open');
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await tick();
  return host.querySelector<HTMLDialogElement>(
    'dialog[aria-labelledby="action-browser-title"][open]',
  )!;
}
export async function chooseCommand(host: Element, id: ActionId) {
  const menu = await openCommands(host);
  const action = menu.querySelector<HTMLButtonElement>(`[data-action="${id}"]`);
  if (!action || action.disabled)
    throw new Error(`Command unavailable: ${id}: ${action?.textContent}`);
  action.click();
  const end = performance.now() + 5000;
  while (host.contains(menu)) {
    if (performance.now() > end) throw new Error('Commands menu did not close');
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await tick();
}
