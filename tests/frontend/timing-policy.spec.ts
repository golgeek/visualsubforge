import { test, expect } from '@playwright/test';
import path from 'node:path';

test('timing families, policy persistence, linked changes and protected navigation', async ({
  page,
}, info) => {
  await page.setViewportSize({ width: 1440, height: 1900 });
  await page.goto('/tests/frontend/scene-quality/?policy');
  const dock = page.getByRole('complementary', { name: 'Review panel' });
  const priority = dock.getByRole('radio', { name: 'Keep spacing' });
  const policy = dock.getByRole('button', {
    name: /Keep (reading speed|spacing) ·/,
  });
  await expect(
    dock.getByRole('button', { name: 'Apply 2 changes' }),
  ).toBeEnabled();
  for (const name of [
    'Gaps and overlaps',
    'Duration and reading speed',
    'Shot changes',
  ])
    await expect(
      dock.getByRole('checkbox', { name: new RegExp(name) }),
    ).toBeChecked();
  const assets = process.env.VSF_TIMING_SCREENSHOTS;
  for (const theme of ['dark', 'light']) {
    await page
      .locator('html')
      .evaluate((el, value) => el.setAttribute('data-theme', value), theme);
    for (const width of [460, 320]) {
      await dock.evaluate((el, w) => {
        el.style.width = w + 'px';
        el.style.maxWidth = 'none';
        el.style.flex = 'none';
      }, width);
      if (
        (await policy.getAttribute('aria-expanded')) !== String(width === 320)
      )
        await policy.click();
      const scope = dock.getByRole('combobox', { name: 'Review scope' });
      expect(
        await scope.evaluate((el) => el.getBoundingClientRect().width),
      ).toBeGreaterThanOrEqual(90);
      expect(
        await scope.evaluate(
          (el) =>
            el.getBoundingClientRect().right <=
            el.closest('aside')!.getBoundingClientRect().right,
        ),
      ).toBe(true);
      if (width === 320)
        expect(
          await priority.evaluate(
            (el) =>
              el.closest('label')!.getBoundingClientRect().right <=
              el.closest('aside')!.getBoundingClientRect().right,
          ),
        ).toBe(true);
      if (assets && info.project.name === 'chromium')
        await dock.screenshot({
          path: path.join(assets, `implemented-${theme}-${width}.png`),
        });
    }
  }
  if ((await policy.getAttribute('aria-expanded')) !== 'true')
    await policy.click();
  await priority.check();
  await expect(
    dock.getByRole('button', { name: 'Untick trade-offs (1)' }),
  ).toBeVisible();
  await dock.getByRole('button', { name: 'Untick trade-offs (1)' }).click();
  await expect(
    dock.getByRole('button', { name: 'Apply 1 change', exact: true }),
  ).toBeEnabled();
  await dock.getByRole('spinbutton').fill('120');
  await dock.getByLabel('Shot change edges').selectOption('nearest');
  const linked = dock.getByRole('checkbox', {
    name: 'Include cue 1→2',
    exact: true,
  });
  await expect(linked).toBeChecked();
  await linked.uncheck();
  await expect(
    dock.getByRole('button', { name: 'Apply 1 change', exact: true }),
  ).toBeEnabled();
  await dock.getByRole('checkbox', { name: /Gaps and overlaps/ }).uncheck();
  await expect(dock.getByText(/· protected/)).toBeVisible();
  await expect
    .poll(() => page.locator('#test').getAttribute('data-timing-request'))
    .toContain('"gaps":false');
  await page.reload();
  await expect(policy).toHaveText(
    /Keep spacing · starts ±120 ms · nearest edge/,
  );
  await expect(
    dock.getByRole('checkbox', { name: /Gaps and overlaps/ }),
  ).toBeChecked();
  await policy.click();
  await page.evaluate(() => sessionStorage.setItem('fail-policy', '1'));
  await dock.getByRole('radio', { name: 'Keep reading speed' }).check();
  await expect(
    page.getByText(/Injected preference write failure/),
  ).toBeVisible();
  await expect(policy).toHaveText(/Keep reading speed/);
  await dock.getByRole('radio', { name: 'Shift', exact: true }).check();
  await expect(dock.getByLabel('Batch operation')).toBeVisible();
  await dock.getByRole('radio', { name: 'Fix', exact: true }).check();
  await expect(
    dock.getByRole('button', { name: 'Edit', exact: true }).first(),
  ).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => sessionStorage.getItem('failed-policy-writes')),
    )
    .toBe('1');
  await dock.getByRole('button', { name: /Issues \d/ }).click();
  await dock.getByRole('button', { name: 'Fix timing issues…' }).click();
  await expect(
    dock.getByRole('radio', { name: 'Fix', exact: true }),
  ).toBeChecked();
  const change = dock.locator('.change .detail').first();
  await change.click();
  await expect(dock.locator('.times').first()).toContainText('Start');
  const edit = dock.getByRole('button', { name: 'Edit', exact: true }).first();
  await expect(edit).toBeVisible();
  await page
    .getByLabel('Subtitle text', { exact: true })
    .fill('Uncommitted wording');
  await edit.click();
  const decision = page.getByRole('group', {
    name: 'Resolve cue draft before navigation',
  });
  await expect(decision).toBeVisible();
  await decision.getByRole('button', { name: 'Cancel navigation' }).click();
  await expect(page.getByLabel('Subtitle text', { exact: true })).toHaveValue(
    'Uncommitted wording',
  );
});
