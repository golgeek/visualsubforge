import { expect, test } from '@playwright/test';

test('the selected cue has distinct endpoint chips and its own explanations', async ({
  page,
}) => {
  await page.goto('/tests/frontend/cue-editor/?explanations');
  await page.locator('[data-cue-id="35"]').click();
  const strip = page.locator('.issue-strip');
  await expect(
    strip.getByRole('button', { name: 'Start near shot change' }),
  ).toBeVisible();
  await expect(
    strip.getByRole('button', { name: 'End near shot change' }),
  ).toBeVisible();
  const launch = strip.getByRole('button', { name: 'All explanations…' });
  await launch.click();
  const dialog = page.getByRole('dialog', { name: 'All cue issues' });
  await expect(dialog.locator('li')).toHaveCount(2);
  await expect(dialog.locator('li')).toHaveText([
    /scene_start detail for cue 35/,
    /scene_end detail for cue 35/,
  ]);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(launch).toBeFocused();
  await launch.click();
  await dialog.getByRole('button', { name: 'Show all issues in file' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(
    page
      .getByRole('complementary', { name: 'Review panel' })
      .getByRole('button', { name: /^Issues/ }),
  ).toHaveAttribute('aria-pressed', 'true');
});
