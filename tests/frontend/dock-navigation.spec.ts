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

for (const choice of [
  'Cancel navigation',
  'Discard draft and navigate',
  'Apply draft and navigate',
]) {
  test(`Timing owns the draft prompt for ${choice}`, async ({ page }) => {
    await page.goto('/tests/frontend/scene-quality/?policy&navigation=70');
    const dock = page.getByRole('complementary', { name: 'Review panel' });
    const apply = dock.locator('.apply');
    const tick = dock.getByRole('checkbox', {
      name: 'Include cue 2',
      exact: true,
    });
    await tick.uncheck();
    await expect(apply).toHaveText('Apply 1 change');
    const edit = dock
      .locator('.manual')
      .getByRole('button', { name: 'Edit', exact: true });
    await expect(edit).toBeVisible();
    const text = page.getByLabel('Subtitle text');
    await text.fill('Protect this draft');
    await edit.click();
    const prompt = dock.getByRole('group', {
      name: 'Resolve cue draft before navigation',
    });
    await expect(prompt.getByRole('button').first()).toBeFocused();
    await expect(
      dock.getByRole('button', { name: 'Timing', exact: true }),
    ).toHaveAttribute('aria-pressed', 'true');
    await page.waitForTimeout(800);
    await expect(page.locator('#test')).not.toHaveAttribute('data-edits', /.+/);
    await prompt.getByRole('button', { name: choice, exact: true }).click();
    await expect(prompt).toHaveCount(0);
    await expect(edit).toBeFocused();
    if (choice === 'Cancel navigation') {
      await expect(text).toHaveValue('Protect this draft');
      await text.focus();
      await text.press('Escape');
      await expect(text).toHaveValue('Cue 1');
      await expect(apply).toHaveText('Apply 1 change');
      await expect(tick).not.toBeChecked();
    } else {
      await expect(text).toHaveValue('Cue 3');
      if (choice.startsWith('Apply'))
        await expect(page.locator('#test')).toHaveAttribute('data-edits', '1');
      else
        await expect(page.locator('#test')).not.toHaveAttribute(
          'data-edits',
          /.+/,
        );
    }
  });
}
