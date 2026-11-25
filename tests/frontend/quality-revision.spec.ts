import { expect, test } from '@playwright/test';

for (const delay of [60, 600]) {
  test(`only edited cues show pending quality at ${delay} ms`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1350, height: 850 });
    await page.goto('/tests/frontend/quality-checking/?large&anchor=' + delay);
    await expect(
      page.getByRole('button', { name: '10000 cues with issues' }),
    ).toBeVisible();
    const edited = page.locator('[data-cue-id="1"] .issue');
    const other = page.locator('[data-cue-id="2"] .issue');
    const finding = page.locator('.finding[data-index="1"]');
    await expect(finding).toBeAttached();
    await page.evaluate(() => {
      const canvas = document.querySelector<HTMLCanvasElement>(
        'canvas[aria-label^="Audio waveform"]',
      )!;
      const context = canvas.getContext('2d')!;
      const rect = context.fillRect.bind(context);
      const stripes: number[][] = [];
      Object.assign(window, { stripes });
      context.fillRect = (x, y, w, h) => {
        if (x === 0 && y === 0) stripes.push([]);
        if (y === 21 && w === 3) stripes.at(-1)?.push(x);
        rect(x, y, w, h);
      };
      const observed: string[] = [];
      Object.assign(window, { observed });
      new MutationObserver((records) => {
        for (const r of records)
          if (
            r.target instanceof Element &&
            r.target.getAttribute('aria-busy') === 'true'
          )
            observed.push(
              r.target.closest('[data-cue-id]')?.getAttribute('data-cue-id') ??
                '',
            );
      }).observe(document.querySelector('.virtual-list')!, {
        subtree: true,
        attributes: true,
        attributeFilter: ['aria-busy'],
      });
    });
    const editor = page.getByLabel('Subtitle text', { exact: true });
    await editor.fill('Resolved');
    await editor.press('Tab');
    if (delay > 150) {
      await expect(edited).toHaveCSS('opacity', '0.65');
      await expect(other).toHaveCSS('opacity', '1');
      await expect(finding).toHaveCSS('opacity', '1');
    }
    await expect(
      page.getByRole('button', { name: '9999 cues with issues' }),
    ).toBeVisible();
    await expect(edited).toHaveAttribute('aria-label', 'No reported issue');
    await expect(other).toHaveAttribute('aria-label', /Warning|Error/);
    const result = await page.evaluate(() => {
      const { stripes, observed } = window as unknown as {
        stripes: number[][];
        observed: string[];
      };
      return { stripes, observed };
    });
    expect(new Set(result.observed)).toEqual(new Set(delay > 150 ? ['1'] : []));
    const last = result.stripes.at(-1)!;
    expect(last.length).toBeGreaterThan(0);
    expect(
      result.stripes.filter((draw) => last.some((x) => !draw.includes(x))),
    ).toHaveLength(0);
  });
}

test('a failed quality read keeps the cue header and rows in place', async ({
  page,
}) => {
  await page.goto(
    '/tests/frontend/quality-checking/?large&anchor=0&errorTrial',
  );
  await expect(
    page.getByRole('button', { name: '10000 cues with issues' }),
  ).toBeVisible();
  const header = page.getByRole('toolbar', { name: 'Cue column headers' });
  const row = page.locator('[data-cue-id="2"]');
  const top = async () => {
    const origin = (await page.locator('.cue-table').boundingBox())!.y;
    return [
      (await header.boundingBox())!.y - origin,
      (await row.boundingBox())!.y - origin,
    ];
  };
  const before = await top();
  const editor = page.getByLabel('Subtitle text', { exact: true });
  await editor.fill('Edited');
  await editor.press('Tab');
  await expect(
    page.getByText('Checks unavailable:', { exact: false }),
  ).toBeVisible();
  expect(await top()).toEqual(before);
});
