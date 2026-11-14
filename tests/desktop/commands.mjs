// Drive the visible discovery UI, preserving the production modal/IPC path.
export async function chooseCommand(browser, id) {
  await browser.waitUntil(() => browser.$('button=Commands').isEnabled(), {
    timeout: 5000,
    timeoutMsg: 'Commands launcher unavailable',
  });
  await browser.execute(() => {
    const button = [...document.querySelectorAll('button')].find(
      (button) => button.textContent.trim() === 'Commands',
    );
    button.focus();
    button.click();
  });
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
