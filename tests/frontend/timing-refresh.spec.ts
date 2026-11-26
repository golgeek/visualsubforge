import { test, expect, type Page } from '@playwright/test';
import { expectStableUnderPointer } from './stable-under-pointer';

async function open(page: Page) {
  await page.setViewportSize({ width: 1440, height: 950 });
  await page.goto('/tests/frontend/scene-quality/?policy&scroll&navigation=0');
  const dock = page.getByRole('complementary', { name: 'Review panel' });
  await expect(
    dock.getByRole('button', { name: 'Apply 160 changes' }),
  ).toBeEnabled();
  await expect(page.locator('.ghost').first()).toBeVisible();
  return dock;
}

test('superseding a delayed timing view never announces an error', async ({
  page,
}) => {
  const dock = await open(page);
  await page.locator('#test').evaluate((el) => {
    el.dataset.delayNextView = '1';
    el.dispatchEvent(new Event('read-timing-page'));
    new MutationObserver(() => {
      const alert = el.querySelector('.timing-review [role=alert]');
      if (alert) el.dataset.seenError = alert.textContent!;
    }).observe(el, { subtree: true, childList: true, characterData: true });
  });
  await page.waitForFunction(
    () => document.querySelector('#test')?.hasAttribute('data-reading'),
    {},
    { polling: 'raf' },
  );
  await dock.getByRole('checkbox', { name: /Gaps and overlaps/ }).uncheck();
  await expect(page.locator('#test')).toHaveAttribute(
    'data-timing-request',
    /"gaps":false/,
  );
  await expect(dock.locator('.apply')).toBeEnabled();
  expect(
    await page.locator('#test').getAttribute('data-seen-error'),
  ).toBeNull();
});

for (const control of ['Include cue 34', 'Gaps and overlaps', 'Keep spacing']) {
  test(
    control + ' keeps its position through a slow preview',
    async ({ page }) => {
      const dock = await open(page);
      if (control === 'Keep spacing')
        await dock
          .getByRole('button', { name: /Keep reading speed ·/ })
          .click();
      const target = dock.getByRole(
        control === 'Keep spacing' ? 'radio' : 'checkbox',
        { name: new RegExp('^' + control + '(?:$| )') },
      );
      await page.locator('#test').evaluate((el) => {
        el.dataset.startDelay = '650';
      });
      const projection = page.locator('.timing-delta').first();
      await expect(projection).toBeVisible();
      const scan = await page.evaluateHandle(() => {
        let present = true;
        let frame = 0;
        function sample() {
          present &&=
            !!document.querySelector('.ghost') &&
            !!document.querySelector('.timing-delta');
          frame = requestAnimationFrame(sample);
        }
        sample();
        return () => {
          cancelAnimationFrame(frame);
          return present;
        };
      });
      await expectStableUnderPointer(
        page,
        target,
        () => target.click(),
        async () => {
          await expect(dock.locator('.apply')).toBeDisabled();
          await expect(dock.locator('.results')).toHaveAttribute(
            'aria-busy',
            'true',
          );
          await expect(page.locator('.ghost.updating').first()).toBeVisible();
          await expect(
            page.locator('.timing-delta.updating').first(),
          ).toBeVisible();
          await expect(dock.locator('.apply')).toBeEnabled();
        },
      );
      expect(await scan.evaluate((f) => f())).toBe(true);
      if (control === 'Include cue 34')
        await expect(dock.locator('.apply')).toHaveText('Apply 159 changes');
    },
  );
}

for (const kind of ['workerBusy', 'policyChanged', 'rejected'] as const) {
  test('timing admission follows the Rust kind: ' + kind, async ({ page }) => {
    const dock = await open(page);
    const message =
      kind === 'rejected'
        ? 'Timing policy changed before preview; try again'
        : 'Different wording for a temporary refusal';
    const host = page.locator('#test');
    await host.evaluate(
      (el, error) => {
        el.dataset.nextStartError = JSON.stringify(error);
      },
      { kind, message },
    );
    await dock.getByRole('checkbox', { name: /Gaps and overlaps/ }).uncheck();
    await expect(host).not.toHaveAttribute('data-next-start-error', /./);
    if (kind === 'rejected') {
      await expect(dock.getByRole('alert')).toHaveText(message);
      await expect(dock.locator('.apply')).toBeDisabled();
    } else {
      await expect(dock.locator('.apply')).toBeEnabled();
      await expect(dock.getByRole('alert')).toHaveCount(0);
    }
  });
}

test('a failed preview stays visible until Retry succeeds', async ({
  page,
}) => {
  const dock = await open(page);
  const host = page.locator('#test');
  await host.evaluate((el) => {
    el.dataset.failStart = '1';
  });
  await dock.getByRole('checkbox', { name: /Gaps and overlaps/ }).uncheck();
  await expect(dock.getByRole('alert')).toHaveText(/Injected preview failure/);
  await expect(dock.locator('.apply')).toBeDisabled();
  await host.evaluate((el) => {
    delete el.dataset.failStart;
  });
  await dock.getByRole('button', { name: 'Retry preview' }).click();
  await expect(dock.locator('.apply')).toBeEnabled();
});

test('a fast replacement swaps directly with immediate Apply protection', async ({
  page,
}) => {
  const dock = await open(page);
  await page.locator('#test').evaluate((el) => {
    el.dataset.startDelay = '30';
    new MutationObserver((records) => {
      if (
        records.some((r) => r.oldValue === 'true') ||
        el.querySelector('.results[aria-busy=true]')
      )
        el.dataset.showedBusy = '1';
    }).observe(el.querySelector('.results')!, {
      attributes: true,
      attributeOldValue: true,
      attributeFilter: ['aria-busy'],
    });
  });
  const cue = dock.getByRole('checkbox', {
    name: 'Include cue 34',
    exact: true,
  });
  const disabled = await cue.evaluate(async (el) => {
    (el as HTMLInputElement).click();
    await Promise.resolve();
    return document.querySelector<HTMLButtonElement>('.apply')!.disabled;
  });
  expect(disabled).toBe(true);
  await expect(dock.locator('.apply')).toHaveText('Apply 159 changes');
  await expect(dock.locator('.apply')).toBeEnabled();
  expect(
    await page.locator('#test').getAttribute('data-showed-busy'),
  ).toBeNull();
});

for (const [kind, fixture] of [
  ['Fix', 'scene-quality/?policy&scroll'],
  ['Shift', 'batch-timing/?idle'],
]) {
  test(
    kind + ' makes zero timing view reads while a ready review is idle',
    async ({ page }) => {
      await page.goto('/tests/frontend/' + fixture);
      await expect(page.locator('.timing-review .apply')).toBeEnabled();
      await expect(page.locator('.ghost').first()).toBeVisible();
      const reads = () =>
        page.locator('#test').getAttribute('data-timing-reads');
      const before = await reads();
      expect(Number(before)).toBeGreaterThan(0);
      await page.waitForTimeout(2100);
      expect(await reads()).toBe(before);
    },
  );
}

test('a Shift preview replacing Fix gates Apply before the single reread returns', async ({
  page,
}) => {
  const dock = await open(page);
  const fixture = page.locator('#test');
  const apply = dock.locator('.apply');
  const before = await apply.boundingBox();
  const reads = Number(await fixture.getAttribute('data-timing-reads'));
  await fixture.evaluate(async (el) => {
    el.dataset.holdReply = 'changes-0';
    el.dispatchEvent(new Event('replace-with-shift'));
  });
  await expect(fixture).toHaveAttribute('data-held-reply', 'changes-0');
  await expect(apply).toBeDisabled();
  expect(await apply.boundingBox()).toEqual(before);
  await fixture.dispatchEvent('release-timing-reply');
  await expect(dock.getByRole('alert')).toContainText('replaced');
  await expect(apply).toBeDisabled();
  expect(Number(await fixture.getAttribute('data-timing-reads'))).toBe(
    reads + 1,
  );
});

test('each document, media, scene and policy change gates Apply immediately and reads one new page', async ({
  page,
}) => {
  await page.goto('/tests/frontend/scene-quality/?refreshInputs');
  await expect(page.locator('#result')).toHaveText('PASS: refresh inputs');
});

test('a delayed release wake from the previous proposal leaves the newer Fix page alone', async ({
  page,
}) => {
  const dock = await open(page);
  const fixture = page.locator('#test');
  await fixture.evaluate((el) => {
    el.dataset.holdReply = 'owner-wake';
    el.dispatchEvent(new Event('replace-with-shift'));
  });
  await expect(fixture).toHaveAttribute('data-held-reply', 'owner-wake');
  await dock
    .getByRole('checkbox', { name: 'Include cue 34', exact: true })
    .uncheck();
  await expect(dock.locator('.apply')).toHaveText('Apply 159 changes');
  await expect(dock.locator('.apply')).toBeEnabled();
  await expect(page.locator('.ghost.updating')).toHaveCount(0);
  const reads = await fixture.getAttribute('data-timing-reads');
  await fixture.dispatchEvent('release-timing-reply');
  await expect(fixture).not.toHaveAttribute('data-held-reply', 'owner-wake');
  await expect(dock.locator('.apply')).toBeEnabled();
  expect(await fixture.getAttribute('data-timing-reads')).toBe(reads);
});
