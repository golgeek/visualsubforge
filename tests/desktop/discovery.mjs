import assert from 'node:assert/strict';
import { join } from 'node:path';

// Actual WebView and owners, generated key events; physical/native input is separate.
export async function actionDiscovery({ browser, until, evidence, record }) {
  const selector = 'dialog[aria-labelledby="action-browser-title"][open]';
  await browser.execute(() => {
    const editor = document.querySelector('[aria-label="Subtitle text"]');
    editor.focus();
    editor.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'k',
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  await until('command palette in the real webview', () =>
    browser.$(selector).isExisting(),
  );
  for (const [width, height] of [
    [1100, 720],
    [1440, 900],
  ]) {
    await browser.setWindowSize(width, height);
    const bounds = await browser.execute((selector) => {
      const dialog = document.querySelector(selector).getBoundingClientRect();
      return {
        left: dialog.left,
        right: dialog.right,
        top: dialog.top,
        bottom: dialog.bottom,
        width: innerWidth,
        height: innerHeight,
      };
    }, selector);
    assert.ok(
      bounds.left >= 0 &&
        bounds.right <= bounds.width &&
        bounds.top >= 0 &&
        bounds.bottom <= bounds.height,
      JSON.stringify(bounds),
    );
    await browser.saveScreenshot(
      join(evidence, `commands-${width}x${height}.png`),
    );
  }
  await browser.execute((selector) => {
    const field = document.querySelector(`${selector} input`);
    field.value = 'Edit selected cue';
    field.dispatchEvent(new Event('input', { bubbles: true }));
  }, selector);
  await until('focus command is available', () =>
    browser.$('[data-action="focusText"]').isEnabled(),
  );
  await browser.execute(() =>
    document.querySelector('[data-action="focusText"]').click(),
  );
  await until('command closes and focuses editor', () =>
    browser.execute(
      (selector) =>
        !document.querySelector(selector) &&
        document.activeElement?.getAttribute('aria-label') === 'Subtitle text',
      selector,
    ),
  );
  await browser.execute(() =>
    document.querySelector('button[aria-label="Keyboard shortcuts"]').click(),
  );
  await until('shortcut sheet', () => browser.$(selector).isExisting());
  assert.match(await browser.$(selector).getText(), /Keyboard shortcuts/);
  await browser.execute(
    (selector) =>
      document
        .querySelector(selector)
        .dispatchEvent(new Event('cancel', { cancelable: true })),
    selector,
  );
  await until(
    'shortcut sheet closes',
    async () => !(await browser.$(selector).isExisting()),
  );
  await browser.setWindowSize(1100, 720);
  await record({
    step: 'action discovery',
    generatedInput: true,
    nativeMenuInput: false,
    screenshots: ['commands-1100x720.png', 'commands-1440x900.png'],
    palette: true,
    shortcutSheet: true,
    focusRestored: true,
  });
}
