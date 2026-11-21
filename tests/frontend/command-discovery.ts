import { tick } from 'svelte';
import type { ActionId } from '../../src/lib/workspace/action-registry';

/** Exercise the visible Commands menu; feature assertions stay in their harness. */
export async function openCommands(host: Element) {
  const launcher =
    host.querySelector<HTMLElement>('canvas[aria-label^="Audio waveform"]') ??
    host.querySelector<HTMLElement>('[data-window-drag]');
  if (!launcher) throw new Error('Workspace focus target unavailable');
  launcher.focus();
  launcher.dispatchEvent(
    new KeyboardEvent('keydown', {
      key: 'k',
      ctrlKey: true,
      bubbles: true,
      cancelable: true,
    }),
  );
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
// Frontend-only path-filter control.
