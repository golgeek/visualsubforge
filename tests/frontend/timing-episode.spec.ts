import { expect, test } from '@playwright/test';
import path from 'node:path';

for (const theme of ['dark', 'light'])
  for (const width of [460, 320]) {
    test(`review the episode at ${width}px in ${theme}`, async ({
      page,
    }, info) => {
      await page.setViewportSize({ width: 1440, height: 1900 });
      const dock = page.getByRole('complementary', { name: 'Review panel' });
      async function open(query = '') {
        await page.goto('/tests/frontend/scene-quality/?episode' + query);
        await expect(
          dock.getByRole('button', { name: /Apply \d+ changes?/ }),
        ).toBeVisible();
        await page
          .locator('html')
          .evaluate(
            (element, theme) => element.setAttribute('data-theme', theme),
            theme,
          );
        await dock.evaluate(
          (element, width) =>
            Object.assign(element.style, {
              width: width + 'px',
              maxWidth: 'none',
              flex: 'none',
            }),
          width,
        );
      }
      async function capture(state: string) {
        const dir =
          process.env.VSF_TIMING_SCREENSHOTS ??
          (theme === 'dark' &&
          width === 460 &&
          ['before-untick', 'after-untick'].includes(state)
            ? process.env.VSF_TIMING_PRECISION_SCREENSHOTS
            : undefined);
        if (dir && info.project.name === 'chromium')
          await dock.screenshot({
            path: path.join(dir, `${state}-${theme}-${width}.png`),
          });
      }
      await open();
      const reason = dock
        .locator('.manual')
        .filter({
          hasText: 'Gap not fixed: this cue would become too fast.',
        })
        .first();
      await expect(reason.locator('small').first()).toContainText(/CPS .*RS /);
      await expect(reason.locator('.budget')).toContainText(
        'Shorten by 28 counted characters',
      );
      expect(
        await reason.evaluate((el) => el.scrollWidth <= el.clientWidth + 1),
      ).toBe(true);
      const duration = dock.locator('.change').filter({
        has: page.getByRole('checkbox', {
          name: 'Include cue 3',
          exact: true,
        }),
      });
      expect(await duration.locator('.effect').textContent()).toBe(
        'duration 500 → 800 ms · CPS 6.0 → 3.8 · RS — → 10.0',
      );
      await dock
        .getByRole('checkbox', { name: 'Include cue 6', exact: true })
        .uncheck();
      await expect(
        dock.getByRole('button', { name: 'Apply 3 changes' }),
      ).toBeEnabled();
      await expect(
        dock.getByRole('checkbox', { name: 'Include cue 7', exact: true }),
      ).toBeChecked();
      await dock
        .getByRole('button', { name: 'Restore default selection' })
        .click();
      await expect(
        dock.getByRole('button', { name: 'Apply 4 changes' }),
      ).toBeEnabled();
      await capture('default');
      await dock.getByRole('button', { name: /Keep reading speed ·/ }).click();
      await dock.getByRole('radio', { name: 'Keep spacing' }).check();
      await dock.getByRole('spinbutton').fill('300');
      await dock.getByLabel('Shot change edges').selectOption('nearest');
      const linked = dock.locator('.change').filter({
        has: page.getByRole('checkbox', {
          name: 'Include cue 4→5',
          exact: true,
        }),
      });
      await expect(linked.locator('.effect')).toContainText(
        'overlap 200 ms → gap 80 ms',
      );
      await expect(linked.locator('.effect')).toContainText('cue 5 CPS');
      const trade = dock.locator('.change').filter({
        has: page.getByRole('checkbox', {
          name: 'Include cue 1',
          exact: true,
        }),
      });
      await expect(trade.locator('.warning')).toHaveCount(2);
      await expect(trade.locator('.warning').first()).toContainText(
        '(limit 20)',
      );
      await capture('spacing');
      await linked.locator('.detail').click();
      await expect(linked.locator('.times')).toContainText('00:00:06,000');
      await capture('expanded');
      const prerequisite = dock.getByRole('checkbox', {
        name: 'Include cue 3',
        exact: true,
      });
      await linked.locator('.detail').click();
      await capture('before-untick');
      await prerequisite.uncheck();
      await expect(
        dock.getByRole('button', { name: 'Apply 4 changes' }),
      ).toBeEnabled();
      await expect(linked.getByRole('checkbox')).toBeChecked();
      await expect(
        dock.getByRole('checkbox', { name: 'Include cue 6→7', exact: true }),
      ).toBeChecked();
      await capture('after-untick');
      await prerequisite.check();
      await expect(
        dock.getByRole('button', { name: 'Apply 5 changes' }),
      ).toBeEnabled();
      await dock
        .getByRole('button', { name: 'Restore default selection' })
        .click();
      await expect(
        dock.getByRole('button', { name: 'Restore default selection' }),
      ).toHaveCount(0);
      const partial = dock.getByRole('checkbox', {
        name: 'Include cue 8→9',
        exact: true,
      });
      await partial.check();
      await expect(
        dock.getByRole('button', { name: 'Apply 6 changes' }),
      ).toBeEnabled();
      await expect(
        dock.getByRole('checkbox', { name: 'Include cue 10', exact: true }),
      ).toBeChecked();
      await expect(dock.getByText('replaces the change to cue 10')).toHaveCount(
        0,
      );
      await capture('partial');
      await open('&incomplete');
      await expect(dock.locator('.scan-reason')).toHaveText(
        'Finish the scene scan to apply timing changes.',
      );
      await expect(dock.locator('footer .apply')).toBeDisabled();
      await expect(
        dock.getByText('No changes for the ticked rules in this scope.'),
      ).toBeVisible();
      await capture('incomplete');
    });
  }

test('deep toggle keeps its page and scroll through the updating view', async ({
  page,
}, info) => {
  await page.setViewportSize({ width: 1440, height: 850 });
  await page.goto('/tests/frontend/scene-quality/?policy&scroll');
  const dock = page.getByRole('complementary', { name: 'Review panel' });
  await dock.evaluate((el) =>
    Object.assign(el.style, { width: '460px', maxWidth: 'none', flex: 'none' }),
  );
  await page
    .locator('html')
    .evaluate((el) => el.setAttribute('data-theme', 'dark'));
  await dock
    .getByRole('navigation', { name: 'Timing proposal pages' })
    .getByRole('button', { name: 'Next' })
    .click();
  const row = dock.getByRole('checkbox', {
    name: 'Include cue 125',
    exact: true,
  });
  await row.evaluate((el) => el.scrollIntoView({ block: 'center' }));
  const scroll = dock.locator('.scroll');
  const top = await scroll.evaluate((el) => el.scrollTop);
  expect(top).toBeGreaterThan(100);
  await row.uncheck();
  const busy = dock.locator('.results[aria-busy="true"]');
  await expect(busy).toBeVisible();
  await expect(row).toBeDisabled();
  await expect(
    dock
      .getByRole('status', { name: '' })
      .filter({ hasText: 'Updating preview…' }),
  ).toBeVisible();
  await expect(row).toBeInViewport();
  const dir = process.env.VSF_TIMING_PRECISION_SCREENSHOTS;
  if (dir && info.project.name === 'chromium')
    await dock.screenshot({ path: path.join(dir, 'mid-list-updating.png') });
  await expect(row).toBeEnabled();
  await expect(row).not.toBeChecked();
  await expect(row).toBeInViewport();
  expect(await scroll.evaluate((el) => el.scrollTop)).toBe(top);
  if (dir && info.project.name === 'chromium')
    await dock.screenshot({ path: path.join(dir, 'mid-list-restored.png') });
});
