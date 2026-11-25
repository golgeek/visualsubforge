import { expect, test, type Locator, type Page } from '@playwright/test';
import { expectStableUnderPointer } from './stable-under-pointer';

async function clickPart(page: Page, part: Locator, padding = false) {
  const box = await part.boundingBox();
  if (!box) throw new Error('Row content has no bounding box');
  await page.mouse.click(
    box.x + box.width / 2,
    box.y + (padding ? 2 : box.height / 2),
  );
}
for (const delay of [70, 1000]) {
  test(
    'Issues row text, measurements and padding select only the cue at ' + delay,
    async ({ page }) => {
      await page.setViewportSize({ width: 1440, height: 950 });
      await page.goto(
        '/tests/frontend/quality-checking/?rowBody&anchor=' + delay,
      );
      const editor = page.getByLabel('Subtitle text', { exact: true });
      for (const [i, selector] of ['small', 'span', ''].entries()) {
        const id = 33 + i;
        const primary = page.getByRole('button', {
          name: new RegExp('^Cue ' + id + ' ·'),
        });
        const row = primary.locator('..');
        await row.evaluate((el) => el.scrollIntoView({ block: 'center' }));
        await expectStableUnderPointer(
          page,
          primary,
          () =>
            clickPart(
              page,
              selector ? row.locator(selector).first() : row,
              !selector,
            ),
          async () => {
            await expect(editor).toHaveValue('Cue ' + id);
            await expect(primary).toBeFocused();
            await expect(row).toHaveAttribute('aria-current', 'true');
            await expect(page.locator('#test')).toHaveAttribute(
              'data-read-cue',
              String(id),
            );
          },
        );
      }
      const related = page
        .locator('.finding')
        .filter({ has: page.getByRole('button', { name: /^Cue 33 ·/ }) })
        .getByRole('button', { name: 'Related cue' });
      await related.click();
      await expect(editor).toHaveValue('Cue 32');
      await expect(related).toBeFocused();
    },
  );
  test(
    'Timing row hits and keyboard controls are independent at ' + delay,
    async ({ page, browserName }) => {
      await page.setViewportSize({ width: 1440, height: 950 });
      await page.goto(
        '/tests/frontend/scene-quality/?policy&scroll&navigation=' + delay,
      );
      const editor = page.getByLabel('Subtitle text', { exact: true });
      for (const [i, selector] of ['.effect', '.number', ''].entries()) {
        const id = 33 + i;
        const primary = page.getByRole('button', {
          name: 'Go to cue ' + id,
          exact: true,
        });
        const row = primary.locator('..');
        await row.evaluate((el) => el.scrollIntoView({ block: 'center' }));
        await expectStableUnderPointer(
          page,
          primary,
          () =>
            clickPart(
              page,
              selector ? row.locator(selector).first() : row,
              !selector,
            ),
          async () => {
            await expect(editor).toHaveValue('Cue ' + id);
            await expect(primary).toBeFocused();
            await expect(page.locator('#test')).toHaveAttribute(
              'data-seek',
              'done',
            );
          },
        );
        await expect(row.getByRole('checkbox')).toBeChecked();
        await expect(row.locator('.times')).toBeHidden();
      }
      const primary = page.getByRole('button', {
        name: 'Go to cue 34',
        exact: true,
      });
      const row = primary.locator('..');
      const checkbox = row.getByRole('checkbox');
      const disclosure = row.getByRole('button', {
        name: 'Timecodes for cue 34',
      });
      // With default macOS WebKit preferences, Option+Tab visits all controls.
      const tab =
        browserName === 'webkit' && process.platform === 'darwin'
          ? 'Alt+Tab'
          : 'Tab';
      await expect(checkbox).toBeEnabled();
      await primary.focus();
      await page.keyboard.press(tab);
      await expect(checkbox).toBeFocused();
      await page.keyboard.press(tab);
      await expect(disclosure).toBeFocused();
      await page.keyboard.press('Enter');
      await expect(disclosure).toHaveAttribute('aria-expanded', 'true');
      await expect(row.locator('.times')).toBeVisible();
      await expect(editor).toHaveValue('Cue 35');
      await checkbox.press('Space');
      await expect(checkbox).not.toBeChecked();
      await expect(editor).toHaveValue('Cue 35');
      await expect(disclosure).toHaveAttribute('aria-expanded', 'true');
      await primary.press('Enter');
      await expect(editor).toHaveValue('Cue 34');
      await expect(disclosure).toHaveAttribute('aria-expanded', 'true');
    },
  );
}
