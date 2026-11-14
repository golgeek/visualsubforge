import '@fontsource-variable/inter/wght.css';
import '@fontsource-variable/jetbrains-mono/wght.css';
import '../../src/lib/ui/tokens.css';
import '../../src/lib/ui/base.css';
import { tick } from 'svelte';

// Opt-in, interactive visual audit. Normal behavioral workflows never pause.
// The capture driver releases each checkpoint after saving its screenshot.
const theme = new URLSearchParams(location.search).get('visual');
const seen = new Set<string>();
let active: Promise<void> | null = null;
async function paint() {
  await tick();
  await document.fonts.ready;
  await Promise.all(
    document
      .getAnimations()
      .filter((animation) => animation instanceof CSSTransition)
      .map((animation) => animation.finished.catch(() => {})),
  );
  await new Promise<void>((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
  );
}
async function capture(dialog: HTMLDialogElement, suffix = '') {
  const title =
    dialog.getAttribute('aria-label') ||
    dialog.querySelector('h2')?.textContent?.trim() ||
    'Dialog';
  const operation = dialog.querySelector<HTMLSelectElement>('select');
  const review = [
    ...dialog.querySelectorAll('section[aria-label], table[aria-label]'),
  ]
    .map((element) => element.getAttribute('aria-label') || '')
    .filter((label) =>
      /^(Font change review|Style change review|Edit style fields|Conversion loss report|Conversion exported|Plugin findings|Plugin changes|Legacy import review|Project created)$/.test(
        label,
      ),
    )
    .join(', ');
  const key = [
    title,
    suffix,
    review,
    dialog.getAttribute('aria-labelledby') === 'resync-title' &&
    dialog.querySelector('table')
      ? 'Timing review'
      : '',
    dialog.getAttribute('aria-labelledby') === 'text-title'
      ? operation?.value
      : '',
  ]
    .filter(Boolean)
    .join(' — ');
  if (seen.has(key)) return;
  seen.add(key);
  await paint();
  if (!dialog.open || !dialog.isConnected)
    throw new Error('Visual checkpoint lost its dialog: ' + key);
  await hold(key);
  const scrollable = [
    dialog,
    ...dialog.querySelectorAll<HTMLElement>('*'),
  ].filter(
    (element) =>
      element.clientHeight > 50 &&
      element.scrollHeight > element.clientHeight + 16 &&
      ['auto', 'scroll'].includes(getComputedStyle(element).overflowY),
  );
  for (const [index, element] of scrollable.slice(0, 2).entries()) {
    const top = element.scrollTop;
    element.scrollTop = element.scrollHeight;
    await paint();
    await hold(`${key} — lower content ${index + 1}`);
    element.scrollTop = top;
  }
}
async function hold(key: string) {
  document.body.dataset.visualCheckpoint = key;
  await new Promise<void>((resolve) =>
    window.addEventListener('vsf-visual-next', () => resolve(), { once: true }),
  );
  delete document.body.dataset.visualCheckpoint;
}
export async function visualCheckpoint() {
  if (theme !== 'light' && theme !== 'dark') return;
  if (active) return active;
  const dialog = [
    ...document.querySelectorAll<HTMLDialogElement>('dialog[open]'),
  ].at(-1);
  if (!dialog) return;
  active = (async () => {
    const root = document.documentElement;
    const previous = root.getAttribute('data-theme');
    const focus = document.activeElement;
    root.dataset.theme = theme;
    try {
      const pages = [
        ...dialog.querySelectorAll<HTMLButtonElement>(
          'nav[aria-label="Settings pages"] button',
        ),
      ];
      const original = pages.find(
        (button) => button.getAttribute('aria-current') === 'page',
      );
      if (pages.length && !seen.has('settings-pages')) {
        seen.add('settings-pages');
        for (const page of pages) {
          page.click();
          await tick();
          await capture(dialog, page.textContent?.trim());
        }
        original?.click();
        await tick();
      } else if (
        dialog.getAttribute('aria-labelledby') === 'text-title' &&
        !seen.has('text-modes') &&
        !dialog.querySelector('select')?.disabled
      ) {
        seen.add('text-modes');
        const select = dialog.querySelector('select')!;
        const original = select.value;
        for (const option of select.options) {
          select.value = option.value;
          select.dispatchEvent(new Event('change', { bubbles: true }));
          await tick();
          await capture(dialog);
        }
        select.value = original;
        select.dispatchEvent(new Event('change', { bubbles: true }));
        await tick();
      } else await capture(dialog);
    } finally {
      if (previous === null) root.removeAttribute('data-theme');
      else root.setAttribute('data-theme', previous);
      if (focus instanceof HTMLElement && focus.isConnected) focus.focus();
    }
  })();
  try {
    await active;
  } finally {
    active = null;
  }
}
