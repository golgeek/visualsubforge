import { expect, test, type Page } from '@playwright/test';

async function checkFromPalette(page: Page) {
  await page.keyboard.press('Control+k');
  await page
    .getByRole('textbox', { name: 'Search actions or cues' })
    .fill('check for updates');
  await page.getByRole('button', { name: /Check for Updates/ }).click();
}

test('first launch, postponed consent and release notice', async ({ page }) => {
  await page.goto('/tests/frontend/start-screen/?preview');
  const region = page.getByRole('region', { name: 'Version updates' });
  await expect(
    region.getByText('Check for new versions automatically?'),
  ).toBeVisible();
  await page.waitForTimeout(3700);
  await expect(page.locator('#test')).not.toHaveAttribute(
    'data-update-checks',
    /./,
  );
  await region.getByRole('button', { name: 'Ask next launch' }).click();
  await expect(region).toHaveCount(0);
  await page.reload();
  await region.getByRole('button', { name: 'Check automatically' }).click();
  await expect(
    region.getByText('VisualSubForge 0.2.0 is available.'),
  ).toBeVisible({ timeout: 7000 });
  await region.getByRole('button', { name: 'View release' }).click();
  await expect(page.locator('#test')).toHaveAttribute(
    'data-opened-version',
    '0.2.0',
  );
  await region.getByRole('button', { name: 'Not now' }).click();
  await expect(page.locator('#test')).toHaveAttribute(
    'data-dismissed-version',
    '0.2.0',
  );
  await expect(region).toHaveCount(0);
});

test('manual check after declining, including rate limiting and a newer release', async ({
  page,
}) => {
  await page.goto('/tests/frontend/start-screen/?preview');
  await page.getByRole('button', { name: "Don't check" }).click();
  for (const [answer, expected] of [
    ['current', 'VisualSubForge is up to date.'],
    [
      'failed',
      "Couldn't check for updates. GitHub’s request limit was reached.",
    ],
    ['newer', 'VisualSubForge 0.2.0 is available.'],
  ]) {
    await page.goto('/tests/frontend/start-screen/?preview&answer=' + answer);
    await expect(
      page.getByRole('button', { name: 'New subtitles' }),
    ).toBeEnabled();
    await checkFromPalette(page);
    await expect(page.getByText(expected!, { exact: true })).toBeVisible();
  }
});
