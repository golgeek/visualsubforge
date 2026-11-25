import { expect, test } from '@playwright/test';

test('table context menu keeps a selected run and retargets outside it', async ({
  page,
}) => {
  await page.goto('/tests/frontend/quality-checking/?editor=70');
  const row = (id: number) => page.locator('[data-cue-id="' + id + '"]');
  const menu = page.getByRole('menu', { name: 'Subtitle actions' });
  await row(2).click();
  await row(3).click({ modifiers: ['Shift'] });
  await row(2).click({ button: 'right' });
  await expect(
    menu.getByRole('menuitem', { name: 'Merge as dialog', exact: true }),
  ).toBeEnabled();
  await expect(row(2)).toHaveAttribute('aria-pressed', 'true');
  await expect(row(3)).toHaveAttribute('aria-pressed', 'true');
  await page.keyboard.press('Escape');
  await row(1).click({ button: 'right' });
  await expect(row(1)).toHaveAttribute('aria-pressed', 'true');
  await expect(row(2)).toHaveAttribute('aria-pressed', 'false');
  await expect(
    menu.getByRole('menuitem', { name: 'Merge with previous', exact: true }),
  ).toBeDisabled();
  await expect(
    menu.getByRole('menuitem', { name: 'Merge with next', exact: true }),
  ).toBeEnabled();
  await page.keyboard.press('Escape');
  await page.getByLabel('Filter cues by text or number').fill('Cue 1');
  await expect(row(2)).toHaveCount(0);
  await row(10).click({ modifiers: ['Shift'] });
  await row(10).press('Shift+F10');
  await expect(
    menu.getByRole('menuitem', { name: 'Merge as dialog', exact: true }),
  ).toBeDisabled();
  await expect(
    menu.getByText('Cues 1 and 10 are not adjacent', { exact: true }).first(),
  ).toBeVisible();
  await expect(
    menu.getByRole('menuitem', { name: 'Delete 2 cues', exact: true }),
  ).toBeEnabled();
  await page.keyboard.press('Escape');
  await expect(row(10)).toBeFocused();
});

for (const route of ['native', 'context']) {
  test(
    route + ' dialogue sends the same revision and IDs in document order',
    async ({ page }) => {
      await page.goto('/tests/frontend/cue-editor/?groupMenu');
      const host = page.locator('#test');
      await expect(host).toHaveAttribute('data-dialogue-enabled', 'false');
      const first = page.locator('[data-cue-id="1"]');
      await page.locator('[data-cue-id="2"]').click();
      await first.click({ modifiers: ['Shift'] });
      await expect(host).toHaveAttribute('data-dialogue-enabled', 'true');
      if (route === 'native') {
        await host.dispatchEvent('fixture-native-dialogue');
      } else {
        await first.press('ContextMenu');
        await page
          .getByRole('menuitem', { name: 'Merge as dialog', exact: true })
          .click();
      }
      await expect(host).toHaveAttribute(
        'data-group-request',
        JSON.stringify({
          action: { kind: 'mergeRun', ids: [1, 2], dialogue: true },
          revision: 0,
        }),
      );
      await expect(
        page.getByLabel('Subtitle text', { exact: true }),
      ).toHaveValue('- One issue\n- Three issues');
      await expect(host).toHaveAttribute('data-dialogue-enabled', 'false');
    },
  );
}

test('right-clicking a selected run commits the draft before the group command', async ({
  page,
}) => {
  await page.goto('/tests/frontend/cue-editor/?groupMenu');
  const first = page.locator('[data-cue-id="1"]');
  await first.click();
  await page.locator('[data-cue-id="2"]').click({ modifiers: ['Shift'] });
  await page
    .getByLabel('Subtitle text', { exact: true })
    .fill('Pending second line');
  await first.click({ button: 'right' });
  await expect(page.locator('[data-cue-id="2"]')).toHaveAttribute(
    'aria-pressed',
    'true',
  );
  await page
    .getByRole('menuitem', { name: 'Merge as dialog', exact: true })
    .click();
  await expect(page.locator('#test')).toHaveAttribute(
    'data-group-request',
    JSON.stringify({
      action: { kind: 'mergeRun', ids: [1, 2], dialogue: true },
      revision: 1,
    }),
  );
});

test('an empty speaker disables dialogue with its reason and still permits plain merge', async ({
  page,
}) => {
  await page.goto('/tests/frontend/cue-editor/?groupMenu');
  const first = page.locator('[data-cue-id="1"]');
  await first.click();
  await page.getByLabel('Subtitle text', { exact: true }).fill('');
  await page.locator('[data-cue-id="2"]').click({ modifiers: ['Shift'] });
  await first.click({ button: 'right' });
  const menu = page.getByRole('menu', { name: 'Subtitle actions' });
  await expect(
    menu.getByRole('menuitem', { name: 'Merge as dialog', exact: true }),
  ).toBeDisabled();
  await expect(
    menu.getByText('Merge as dialog needs text in both cues'),
  ).toBeVisible();
  await expect(
    menu.getByRole('menuitem', { name: 'Merge', exact: true }),
  ).toBeEnabled();
  await expect(page.locator('#test')).toHaveAttribute(
    'data-dialogue-enabled',
    'false',
  );
});

test('the menu shows a format refusal returned by Rust', async ({ page }) => {
  await page.goto('/tests/frontend/cue-editor/?groupMenu&groupReply=webvtt');
  const first = page.locator('[data-cue-id="1"]');
  await first.click();
  await page.locator('[data-cue-id="2"]').click({ modifiers: ['Shift'] });
  await first.click({ button: 'right' });
  const menu = page.getByRole('menu', { name: 'Subtitle actions' });
  await expect(
    menu.getByRole('menuitem', { name: 'Merge', exact: true }),
  ).toBeDisabled();
  await expect(
    menu.getByText(/WebVTT merge would change markup scope/).first(),
  ).toBeVisible();
  await expect(
    menu.getByRole('menuitem', { name: 'Delete 2 cues' }),
  ).toBeEnabled();
});

test('a failed group read leaves the menu closed and shows the error', async ({
  page,
}) => {
  await page.goto('/tests/frontend/cue-editor/?groupMenu&groupReply=failure');
  const first = page.locator('[data-cue-id="1"]');
  await first.click();
  await page.locator('[data-cue-id="2"]').click({ modifiers: ['Shift'] });
  await first.click({ button: 'right' });
  await expect(page.locator('footer')).toContainText(
    'Cannot read accepted cue selection',
  );
  await expect(
    page.getByRole('menu', { name: 'Subtitle actions' }),
  ).toHaveCount(0);
});
