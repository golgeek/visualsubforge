import { expect, test } from '@playwright/test';
import { expectStableUnderPointer } from './stable-under-pointer';

for (const delay of [70, 1000]) {
  test(
    'a deep finding keeps its place through selection and resolution at ' +
      delay +
      ' ms',
    async ({ page }) => {
      await page.setViewportSize({ width: 1350, height: 850 });
      await page.goto('/tests/frontend/quality-checking/?anchor=' + delay);
      const dock = page.getByRole('complementary', { name: 'Review panel' });
      const row = dock.getByRole('button', { name: /^Cue 35 ·/ });
      await row.evaluate((el) => el.scrollIntoView({ block: 'center' }));
      await expectStableUnderPointer(
        page,
        row,
        () => row.click(),
        async () => {
          await expect(row).toBeFocused();
          await expect(dock.locator('.findings')).toHaveAttribute(
            'aria-busy',
            'false',
          );
          if (delay === 1000)
            await expect(page.locator('.activity-slot')).toBeVisible();
          await expect(row.locator('..')).toHaveAttribute(
            'aria-current',
            'true',
          );
          await expect(page.locator('#test')).toHaveAttribute(
            'data-read-state',
            'done',
          );
        },
      );
      const top = (await row.boundingBox())!.y;
      await page
        .getByLabel('Subtitle text', { exact: true })
        .fill('Resolved finding');
      await expect(row).toBeVisible();
      expect(Math.abs((await row.boundingBox())!.y - top)).toBeLessThanOrEqual(
        1,
      );
      await page.getByLabel('Subtitle text', { exact: true }).press('Tab');
      if (delay === 1000) {
        await expect(dock.locator('.findings')).toHaveAttribute(
          'aria-busy',
          'true',
        );
        await expect(row).toBeEnabled();
      }
      await expect(row).toHaveCount(0);
      const next = dock.getByRole('button', { name: /^Cue 36 ·/ });
      expect(Math.abs((await next.boundingBox())!.y - top)).toBeLessThanOrEqual(
        1,
      );
    },
  );
}
