import { expect, test } from '@playwright/test';
import { expectStableUnderPointer } from './stable-under-pointer';

for (const delay of [70, 1000]) {
  test(`whole-file cue navigation keeps the review through a ${delay} ms seek`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1440, height: 950 });
    await page.goto(
      '/tests/frontend/scene-quality/?policy&scroll&navigation=' + delay,
    );
    const dock = page.getByRole('complementary', { name: 'Review panel' });
    const apply = dock.locator('footer .apply');
    const tick = dock.getByRole('checkbox', {
      name: 'Include cue 34',
      exact: true,
    });
    await expect(apply).toHaveText('Apply 160 changes');
    await tick.uncheck();
    await expect(apply).toHaveText('Apply 159 changes');
    const cue = dock.getByRole('button', { name: 'Go to cue 33', exact: true });
    const detail = cue.locator('..').locator('.detail');
    await detail.click();
    await cue.evaluate((el) => el.scrollIntoView({ block: 'center' }));
    await expectStableUnderPointer(
      page,
      cue,
      () => cue.click(),
      async () => {
        if (delay === 1000) {
          await expect(page.locator('#test')).toHaveAttribute(
            'data-seek',
            'pending',
          );
          await expect(apply).toBeDisabled();
          await expect(page.locator('.activity-slot')).toBeVisible();
        }
        await expect(page.locator('#test')).toHaveAttribute(
          'data-seek',
          'done',
        );
        await expect(
          dock.getByRole('button', { name: 'Timing', exact: true }),
        ).toHaveAttribute('aria-pressed', 'true');
        await expect(cue).toBeFocused();
        await expect(apply).toBeEnabled();
      },
    );
    await expect(page.getByLabel('Subtitle text')).toHaveValue('Cue 33');
    await expect(tick).not.toBeChecked();
    await expect(apply).toHaveText('Apply 159 changes');
    const offset = () =>
      cue.evaluate(
        (el) =>
          el.getBoundingClientRect().top -
          el.closest('.scroll')!.getBoundingClientRect().top,
      );
    const top = await offset();
    await dock.getByRole('button', { name: /^Issues/ }).click();
    await dock.getByRole('button', { name: 'Timing', exact: true }).click();
    await expect(apply).toHaveText('Apply 159 changes');
    await expect(tick).not.toBeChecked();
    await expect(detail).toHaveAttribute('aria-expanded', 'true');
    expect(Math.abs((await offset()) - top)).toBeLessThanOrEqual(1);
    await dock.getByLabel('Review scope').selectOption('current');
    await expect(apply).toHaveText('Apply 1 change');
    await expect(
      dock.getByRole('button', { name: 'Go to cue 33', exact: true }),
    ).toBeVisible();
    await page.locator('[data-cue-id="34"]').click();
    await expect(
      dock.getByRole('button', { name: 'Go to cue 34', exact: true }),
    ).toBeVisible();
    await expect(
      dock.getByRole('checkbox', { name: 'Include cue 34', exact: true }),
    ).toBeChecked();
  });
}

test('Statistics outlier follows its cue on the same tab', async ({ page }) => {
  await page.goto('/tests/frontend/scene-quality/?policy&navigation=1000');
  const dock = page.getByRole('complementary', { name: 'Review panel' });
  const tab = dock.getByRole('button', { name: 'Statistics', exact: true });
  await tab.click();
  const cue = dock.getByRole('button', { name: /^Cue 8 ·/ });
  await expectStableUnderPointer(
    page,
    cue,
    () => cue.click(),
    async () => {
      await expect(page.locator('#test')).toHaveAttribute('data-seek', 'done');
      await expect(cue).toBeFocused();
      await expect(tab).toHaveAttribute('aria-pressed', 'true');
    },
  );
  await expect(page.getByLabel('Subtitle text')).toHaveValue('Cue 8');
});

for (const tab of ['Issues', 'Timing', 'Statistics']) {
  test(`${tab} mouse navigation commits the focused draft and follows the clicked cue`, async ({
    page,
  }) => {
    await page.goto('/tests/frontend/scene-quality/?policy&navigation=180');
    const dock = page.getByRole('complementary', { name: 'Review panel' });
    const tabButton = dock
      .getByRole('button', { name: new RegExp('^' + tab) })
      .first();
    await tabButton.click();
    const target =
      tab === 'Timing'
        ? dock
            .locator('.manual')
            .getByRole('button', { name: 'Edit', exact: true })
        : dock.getByRole('button', {
            name: tab === 'Issues' ? /^Next cue/ : /^Cue 8 ·/,
          });
    await expect(target).toBeEnabled();
    const text = page.getByLabel('Subtitle text');
    await text.fill('Pending subtitle');
    await target.click();
    await expect(text).toHaveValue(tab === 'Timing' ? 'Cue 3' : 'Cue 8');
    await expect(page.locator('#test')).toHaveAttribute(
      'data-edit-attempts',
      '1',
    );
    await expect(page.locator('#test')).toHaveAttribute('data-edits', '1');
    await expect(page.locator('#test')).toHaveAttribute('data-seek', 'done');
    await expect(tabButton).toHaveAttribute('aria-pressed', 'true');
    await expect(target).toBeFocused();
  });
}

test('a rejected dock commit leaves the selected cue and its inline reason', async ({
  page,
}) => {
  await page.goto('/tests/frontend/scene-quality/?policy&navigation=180');
  const dock = page.getByRole('complementary', { name: 'Review panel' });
  await dock.getByRole('button', { name: /^Issues/ }).click();
  const host = page.locator('#test');
  await host.evaluate((el) => {
    el.dataset.rejectEdit = '1';
  });
  const text = page.getByLabel('Subtitle text');
  await text.fill('Keep this draft');
  await dock.getByRole('button', { name: /^Next cue/ }).click();
  await expect(text).toHaveValue('Keep this draft');
  await expect(text).toHaveAttribute('aria-invalid', 'true');
  await expect(page.locator('#cue-commit-error')).toContainText(
    'Rejected navigation draft',
  );
  await expect(page.locator('[data-cue-id="1"]')).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await expect(host).toHaveAttribute('data-edit-attempts', '1');
  await expect(host).not.toHaveAttribute('data-seeks', /.+/);
});
