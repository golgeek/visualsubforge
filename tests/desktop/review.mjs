// Native workflow helpers operate production dock controls only. Read-only
// status observes acceptance; no edit/accept IPC is synthesized here.
export async function setReviewScope(browser, value) {
  await browser.execute((value) => {
    const input = document.querySelector('[aria-label="Review scope"]');
    if (!input) throw new Error('Review scope unavailable');
    input.value = value;
    input.dispatchEvent(new Event('change', { bubbles: true }));
  }, value);
}
export async function closeReview(browser, until) {
  const panel = await browser.$('[aria-label="Review panel"]');
  await until('review acknowledgment finished', () =>
    panel.$('button=Close').then((button) => button.isEnabled()),
  );
  await (await panel.$('button=Close')).click();
  await until('review dock closed', async () => !(await panel.isExisting()));
}
export async function acceptReview(browser, status, until) {
  const revision = (await status()).revision;
  const button = await browser.$('.timing-review .apply');
  await until('current timing subset', () => button.isEnabled());
  await button.click();
  await until(
    'timing acceptance revision',
    async () => (await status()).revision === revision + 1,
  );
  await closeReview(browser, until);
}
