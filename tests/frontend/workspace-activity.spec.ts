import { expect, test } from '@playwright/test';

for (const delay of [35, 210, 1000]) {
  test(`workspace activity for a ${delay} ms seek`, async ({ page }) => {
    await page.goto('/tests/frontend/scene-quality/?activity');
    const slot = page.locator('footer .activity-slot');
    const host = page.locator('#test');
    await expect(host).toHaveAttribute('data-reads', /[1-9]/);
    await expect(slot).toHaveCSS('visibility', 'hidden');
    const rects = () =>
      slot.evaluate((el) =>
        [el, el.parentElement!].map((e) => e.getBoundingClientRect().toJSON()),
      );
    const before = await rects();
    await host.evaluate((el, delay) => {
      el.dataset.delay = String(delay);
      new MutationObserver(() => {
        const shown =
          getComputedStyle(el.querySelector('.activity-slot')!).visibility ===
          'visible';
        el.dataset[shown ? 'shown' : 'hidden'] = String(performance.now());
      }).observe(el.querySelector('.activity-slot')!, {
        attributes: true,
        attributeFilter: ['class'],
      });
    }, delay);
    await page.locator('.cue-row').nth(1).click();
    await expect(host).toHaveAttribute('data-seeks', '1');
    if (delay > 150) {
      await expect(slot).toBeVisible();
      expect(await rects()).toEqual(before);
    }
    await expect(page.locator('main')).toHaveAttribute('aria-busy', 'false');
    await expect(slot).toBeHidden();
    expect(await rects()).toEqual(before);
    const times = await host.evaluate((el) => [
      Number(el.dataset.shown),
      Number(el.dataset.hidden),
    ]);
    if (delay < 150) expect(times[0]).toBeNaN();
    else expect(times[1]! - times[0]!).toBeGreaterThanOrEqual(298);
    if (delay === 1000) {
      await host.evaluate((el) => {
        delete el.dataset.shown;
        el.dataset.readDelay = '250';
      });
      const reads = Number(await host.getAttribute('data-reads'));
      await page.waitForTimeout(5000);
      expect(Number(await host.getAttribute('data-reads'))).toBeGreaterThan(
        reads,
      );
      await expect(host).not.toHaveAttribute('data-shown', /.+/);
    }
  });
}

test('settle and overlapping activity lifetimes with a fake clock', async ({
  page,
}) => {
  await page.goto('/tests/frontend/workspace-activity/settle.svelte.ts');
  await page.clock.install();
  const unit = await page.evaluateHandle(async () => {
    const path = '/tests/frontend/workspace-activity/settle.svelte.ts';
    const module: typeof import('./workspace-activity/settle.svelte') =
      await import(path);
    return module.observe();
  });
  await unit.evaluate((u) => u.pending(true));
  await page.clock.runFor(75);
  await unit.evaluate((u) => u.pending(false));
  await page.clock.runFor(150);
  expect(await unit.evaluate((u) => u.updating())).toBe(false);
  await unit.evaluate((u) => u.pending(true));
  await page.clock.runFor(150);
  expect(await unit.evaluate((u) => u.updating())).toBe(true);
  await unit.evaluate((u) => u.pending(false));
  expect(await unit.evaluate((u) => u.updating())).toBe(false);
  await unit.evaluate((u) => {
    u.pending(true);
    u.dispose();
  });
  await page.clock.runFor(160);
  expect(await unit.evaluate((u) => u.updating())).toBe(false);
  const ends = await unit.evaluateHandle((u) => [
    u.activity.begin(),
    u.activity.begin(),
  ]);
  await ends.evaluate((e) => {
    e[0]!();
    e[0]!();
  });
  await page.clock.runFor(155);
  expect(await unit.evaluate((u) => u.activity.visible)).toBe(true);
  await ends.evaluate((e) => e[1]!());
  await unit.evaluate((u) => u.activity.dispose());
  await page.clock.runFor(300);
  expect(await unit.evaluate((u) => u.activity.visible)).toBe(false);
});
