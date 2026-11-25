import { expect, test, type Locator } from '@playwright/test';

function arrow(target: Locator, key: string, shiftKey = false) {
  return target.evaluate(
    (el, init) => {
      (el as HTMLElement).focus();
      return el.dispatchEvent(
        new KeyboardEvent('keydown', {
          ...init,
          altKey: true,
          bubbles: true,
          cancelable: true,
        }),
      );
    },
    { key, shiftKey },
  );
}

test('macOS word jumps and selections reach editable fields, and seeking still works from the cue list and waveform', async ({
  page,
}) => {
  await page.addInitScript(() =>
    Object.defineProperty(navigator, 'platform', { value: 'MacIntel' }),
  );
  await page.goto('/tests/frontend/cue-editor/?groupMenu');
  const host = page.locator('#test');
  await expect(host).toHaveAttribute('data-dialogue-enabled', 'false');
  const editor = page.getByLabel('Subtitle text', { exact: true });
  const filter = page.getByLabel('Filter cues by text or number');
  for (const target of [editor, filter]) {
    for (const key of ['ArrowLeft', 'ArrowRight']) {
      for (const shift of [false, true])
        expect(await arrow(target, key, shift)).toBe(true);
    }
  }
  await expect(host).not.toHaveAttribute('data-last-seek', /.+/);
  expect(await arrow(page.locator('[data-cue-id="1"]'), 'ArrowRight')).toBe(
    false,
  );
  await expect(host).toHaveAttribute('data-last-seek', '3');
  expect(
    await arrow(
      page.locator('canvas[aria-label^="Audio waveform"]'),
      'ArrowLeft',
    ),
  ).toBe(false);
  await expect(host).toHaveAttribute('data-last-seek', '2');
});
