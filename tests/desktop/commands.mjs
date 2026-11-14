// Drive the visible discovery UI, preserving the production modal/IPC path.
export async function chooseCommand(browser, id, { keyboard = false } = {}) {
  await browser.waitUntil(
    () =>
      browser
        .$('main')
        .getAttribute('aria-busy')
        .then((busy) => busy === 'false'),
    { timeout: 5000 },
  );
  await browser.execute((keyboard) => {
    if (!keyboard)
      document.querySelector('canvas[aria-label^="Audio waveform"]').focus();
    document.activeElement.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'k',
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
  }, keyboard);
  const selector = `dialog[aria-labelledby="action-browser-title"][open] [data-action="${id}"]`;
  await browser.waitUntil(() => browser.$(selector).isEnabled(), {
    timeout: 5000,
    timeoutMsg: `Command unavailable: ${id}`,
  });
  await browser.execute(
    (selector) => document.querySelector(selector).click(),
    selector,
  );
  await browser.waitUntil(
    async () =>
      !(await browser
        .$('dialog[aria-labelledby="action-browser-title"][open]')
        .isExisting()),
    { timeout: 5000, timeoutMsg: 'Commands menu did not close' },
  );
}
