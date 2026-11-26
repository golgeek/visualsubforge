import { expect, test, type Page } from '@playwright/test';

async function recordStripes(page: Page) {
  await page.addInitScript(() => {
    const draws: number[][] = [];
    Object.assign(window, { stripes: draws });
    // Follow painted stripes through 1:1 layer copies. Record the stripe
    // list on presentation, including any frame that presents an empty layer.
    const layers = new WeakMap<HTMLCanvasElement, number[]>();
    const proto = CanvasRenderingContext2D.prototype;
    const rect = proto.fillRect;
    proto.fillRect = function (x, y, w, h) {
      if (x === 0 && y === 0) layers.set(this.canvas, []);
      if (y === 21 && w === 3) layers.get(this.canvas)?.push(x);
      rect.call(this, x, y, w, h);
    };
    const copy = proto.drawImage;
    proto.drawImage = function (
      source: CanvasImageSource,
      ...coords: number[]
    ) {
      Reflect.apply(copy, this, [source, ...coords]);
      if (
        !(source instanceof HTMLCanvasElement) ||
        coords[0] !== 0 ||
        coords[1] !== 0
      )
        return;
      const stripes = [...(layers.get(source) ?? [])];
      layers.set(this.canvas, stripes);
      if (this.canvas.matches('canvas[aria-label^="Audio waveform"]'))
        draws.push(stripes);
    };
  });
}

for (const delay of [60, 600]) {
  test(`only edited cues show pending quality at ${delay} ms`, async ({
    page,
  }) => {
    await recordStripes(page);
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
      (window as unknown as { stripes: number[][] }).stripes.length = 0;
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

test('deleted cues dim their own and related findings during a slow recheck', async ({
  page,
}) => {
  await page.goto('/tests/frontend/quality-checking/?anchor=1000&deleteTrial');
  const findings = page.locator('.finding');
  await expect(findings).toHaveCount(70);
  await page.getByRole('button', { name: 'Delete cue', exact: true }).click();
  await expect(findings.nth(0)).toHaveCSS('opacity', '0.6');
  await expect(findings.nth(1)).toHaveCSS('opacity', '0.6');
  await expect(findings.nth(2)).toHaveCSS('opacity', '1');
  await expect(findings).toHaveCount(68);
  await expect(findings.first()).toHaveCSS('opacity', '1');
});

test('a failed quality read retains stripes, markers, metrics and layout until recovery', async ({
  page,
}) => {
  await recordStripes(page);
  await page.goto('/tests/frontend/quality-checking/?large&anchor=0');
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
  const stripes = () =>
    page.evaluate(() =>
      (window as unknown as { stripes: number[][] }).stripes.at(-1),
    );
  const markers = () =>
    page
      .locator('[data-cue-id] .issue')
      .evaluateAll((els) =>
        els.map((el) => [
          el.closest('[data-cue-id]')!.getAttribute('data-cue-id'),
          el.getAttribute('aria-label'),
        ]),
      );
  const metric = page.getByLabel('Accepted cue metrics', { exact: true });
  await expect(metric).toBeVisible();
  const beforeMetrics = await metric.innerText();
  const beforeMarkers = await markers();
  await expect
    .poll(async () => (await stripes())?.length ?? 0)
    .toBeGreaterThan(1);
  const beforeStripes = await stripes();
  expect(beforeStripes?.length).toBeGreaterThan(1);
  await page
    .locator('#test')
    .evaluate((el) => el.setAttribute('data-fail-read', ''));
  const expectRetained = async () => {
    await expect(
      page.getByText('Checks unavailable:', { exact: false }),
    ).toBeVisible();
    expect(await top()).toEqual(before);
    expect(await markers()).toEqual(beforeMarkers);
    expect(await stripes()).toEqual(beforeStripes);
    await expect(metric).toHaveText(beforeMetrics, { useInnerText: true });
    await expect(
      page.getByRole('button', { name: 'Next cue · F4' }),
    ).toBeDisabled();
  };
  await expectRetained();
  const editor = page.getByLabel('Subtitle text', { exact: true });
  await editor.fill('Edited');
  await editor.press('Tab');
  await expectRetained();
  await page
    .locator('#test')
    .evaluate((el) => el.removeAttribute('data-fail-read'));
  await expect(
    page.getByRole('button', { name: '9999 cues with issues' }),
  ).toBeVisible();
  await expect(
    page.getByText('Checks unavailable:', { exact: false }),
  ).toBeHidden();
  await expect(page.locator('[data-cue-id="1"] .issue')).toHaveAttribute(
    'aria-label',
    'No reported issue',
  );
  await expect.poll(stripes).toEqual(beforeStripes!.slice(1));
  await expect(
    page.getByRole('button', { name: 'Next cue · F4' }),
  ).toBeEnabled();
});
