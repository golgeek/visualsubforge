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
        const dir = process.env.VSF_TIMING_SCREENSHOTS;
        if (dir && info.project.name === 'chromium')
          await dock.screenshot({
            path: path.join(dir, `${state}-${theme}-${width}.png`),
          });
      }
      await open();
      const reason = dock
        .locator('.manual')
        .filter({ hasText: 'Gap not fixed: this cue would become too fast.' })
        .first();
      await expect(reason.locator('small').first()).toContainText(/CPS .*RS /);
      await expect(reason.locator('.budget')).toContainText(
        'Shorten by 28 counted characters',
      );
      expect(
        await reason.evaluate((el) => el.scrollWidth <= el.clientWidth + 1),
      ).toBe(true);
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
      const dependent = linked.getByRole('checkbox');
      await prerequisite.uncheck();
      await expect(dependent).not.toBeChecked();
      await capture('cascade');
      await dependent.check();
      await expect(prerequisite).toBeChecked();
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
      const replacement = dock.getByRole('checkbox', {
        name: 'Include cue 10',
        exact: true,
      });
      await expect(
        dock.getByText('replaces the change to cue 10'),
      ).toBeVisible();
      await partial.check();
      await expect(replacement).not.toBeChecked();
      await expect(dock.getByText('off: cue 8 partial ticked')).toBeVisible();
      await capture('partial');
      await replacement.check();
      await expect(partial).not.toBeChecked();
      await expect(dock.getByText('off: cue 8 partial ticked')).toHaveCount(0);
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
