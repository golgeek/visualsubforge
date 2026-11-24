import { expect, test, type Locator } from '@playwright/test';
import { expectStableUnderPointer } from './stable-under-pointer';

async function inside(target: Locator, dock: Locator) {
  const outer = await dock.boundingBox();
  const box = await target.boundingBox();
  expect(outer).toBeTruthy();
  expect(box).toBeTruthy();
  expect(box!.y).toBeGreaterThanOrEqual(outer!.y - 1);
  expect(box!.y + box!.height).toBeLessThanOrEqual(
    outer!.y + outer!.height + 1,
  );
}
async function useMinimumPane(dock: Locator) {
  await dock.evaluate((el) =>
    Object.assign(el.style, { width: '320px', height: '160px', flex: 'none' }),
  );
}
for (const minimum of [false, true]) {
  const size = minimum ? '160 px pane' : 'restored window';
  test(
    'Issues shows a finding and keeps its tabs while scrolling in the ' + size,
    async ({ page }) => {
      await page.setViewportSize({ width: 1100, height: 720 });
      await page.goto(
        '/tests/frontend/quality-checking/?anchor=' + (minimum ? 1000 : 70),
      );
      const dock = page.getByRole('complementary', { name: 'Review panel' });
      const first = dock.locator('.finding button').first();
      await expect(first).toBeAttached();
      if (minimum) await useMinimumPane(dock);
      const details = dock.locator('details').filter({
        has: dock.page().getByText('Check details', { exact: true }),
      });
      await expect(details).not.toHaveAttribute('open', '');
      await inside(first, dock);
      const findings = dock.locator('.findings');
      await findings.evaluate((el) => (el.scrollTop = el.scrollHeight));
      expect(await findings.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
      await inside(dock.getByLabel('Review tools'), dock);
      await inside(dock.getByLabel('Issue rule'), dock);
      const row = dock.getByRole('button', { name: /^Cue 35 ·/ });
      await row.click();
      const host = page.locator('#test');
      await expect(host).toHaveAttribute('data-read-state', 'done');
      await expect(row).toBeFocused();
      await host.evaluate((el) => el.removeAttribute('data-read-state'));
      await expectStableUnderPointer(
        page,
        dock.getByRole('button', { name: /^Issues \d/ }),
        async () => {
          await dock.getByLabel('Issue rule').selectOption('gap');
        },
        async () => {
          await expect(page.locator('#test')).toHaveAttribute(
            'data-read-state',
            'done',
          );
          await inside(dock.getByLabel('Review tools'), dock);
        },
      );
    },
  );
  test(
    'Timing keeps Apply and the tabs inside the ' + size,
    async ({ page }) => {
      await page.setViewportSize({ width: 1100, height: 720 });
      await page.goto('/tests/frontend/scene-quality/?episode');
      const dock = page.getByRole('complementary', { name: 'Review panel' });
      const apply = dock.getByRole('button', { name: /Apply \d+ changes?/ });
      await expect(apply).toBeEnabled();
      if (minimum) await useMinimumPane(dock);
      const list = dock.locator('.scroll');
      await list.evaluate((el) => (el.scrollTop = el.scrollHeight));
      expect(await list.evaluate((el) => el.scrollTop)).toBeGreaterThan(0);
      await inside(apply, dock);
      await inside(dock.getByLabel('Review tools'), dock);
    },
  );
}
