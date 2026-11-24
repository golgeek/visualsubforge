import { expect, test, type Page } from '@playwright/test';
import { expectStableUnderPointer } from './stable-under-pointer';

async function openCue(page: Page, delay: number, filtered: boolean) {
  await page.setViewportSize({ width: 1100, height: 720 });
  await page.goto('/tests/frontend/quality-checking/?editor=' + delay);
  const row = page.locator('[data-cue-id="35"]');
  if (filtered)
    await page.getByRole('button', { name: '70 cues with issues' }).click();
  await page.getByRole('button', { name: /^Cue 35 ·/ }).click();
  await expect(row).toHaveAttribute('aria-pressed', 'true');
  await expect(
    page.getByLabel('Accepted cue metrics', { exact: true }),
  ).toBeVisible();
  return row;
}
for (const delay of [70, 1000]) {
  test(
    'the deep cue-table row stays under the pointer through a commit at ' +
      delay +
      ' ms',
    async ({ page }) => {
      const row = await openCue(page, delay, false);
      const editor = page.getByRole('textbox', { name: 'Subtitle text' });
      const revision = Number(
        await page.locator('#test').getAttribute('data-read-revision'),
      );
      await expectStableUnderPointer(
        page,
        row,
        async () => {
          await editor.fill('Cue 35!');
          await editor.press('Tab');
        },
        async () => {
          await expect(page.locator('#test')).toHaveAttribute(
            'data-edits',
            '1',
          );
          await expect(
            page.getByLabel('Accepted cue metrics', { exact: true }),
          ).toBeVisible();
          await expect(page.locator('#test')).toHaveAttribute(
            'data-read-revision',
            String(revision + 1),
          );
          await expect(row).toHaveAttribute('aria-pressed', 'true');
        },
      );
    },
  );
  test(
    'filtered edits keep the viewport and pin the resolved cue at ' +
      delay +
      ' ms',
    async ({ page }) => {
      const row = await openCue(page, delay, true);
      const editor = page.getByRole('textbox', { name: 'Subtitle text' });
      const counts = page.locator('.cue-count');
      const initialRevision = Number(
        await page.locator('#test').getAttribute('data-read-revision'),
      );
      for (const [i, value] of ['Cue 35!', 'Resolved'].entries()) {
        await editor.fill(value);
        await editor.press('Tab');
        await expect(page.locator('#test')).toHaveAttribute(
          'data-edits',
          String(i + 1),
        );
        if (delay === 1000) {
          await expect(page.locator('.virtual-list')).toHaveAttribute(
            'aria-busy',
            'true',
          );
          await expect(page.locator('.activity-slot')).toBeVisible();
        }
        for (let sample = 0; sample < 3; sample++) {
          // Sample the visible count during the held production read.
          expect(await counts.textContent()).toContain('70 / 70');
          if (delay === 1000) await page.waitForTimeout(100);
        }
        await expect(page.locator('#test')).toHaveAttribute(
          'data-read-revision',
          String(initialRevision + i + 1),
        );
        await expect(row).toHaveAttribute('aria-pressed', 'true');
      }
      await expect(row.getByLabel('Resolved', { exact: true })).toBeVisible();
      await page.locator('[data-cue-id="34"]').click();
      await expect(row).toHaveCount(0);
      await expect(counts).toContainText('69 / 70');
    },
  );
  test(
    'textarea and source-line gutter keep their geometry at ' + delay + ' ms',
    async ({ page }) => {
      await openCue(page, delay, false);
      const editor = page.getByRole('textbox', { name: 'Subtitle text' });
      const geometry = () =>
        editor.evaluate((el) => {
          const box = el.getBoundingClientRect();
          return [
            box.x,
            box.y,
            box.width,
            box.height,
            el.clientWidth,
            el.scrollHeight,
          ] as const;
        });
      const revision = Number(
        await page.locator('#test').getAttribute('data-read-revision'),
      );
      const nearWrap = await editor.evaluate((el) => {
        const style = getComputedStyle(el);
        const ctx = document.createElement('canvas').getContext('2d')!;
        ctx.font = style.font;
        const width =
          el.clientWidth -
          parseFloat(style.paddingLeft) -
          parseFloat(style.paddingRight);
        return 'M'.repeat(Math.floor(width / ctx.measureText('M').width) - 1);
      });
      // Force overflow so scrollHeight records wrapped lines as well as the box.
      const text = Array(8).fill(nearWrap).join('\n');
      await editor.fill(text);
      await editor.press('Tab');
      await expect(page.locator('#test')).toHaveAttribute('data-edits', '1');
      await expect(
        page.getByLabel('Accepted cue metrics', { exact: true }),
      ).toBeVisible();
      await expect(page.locator('#test')).toHaveAttribute(
        'data-read-revision',
        String(revision + 1),
      );
      const before = await geometry();
      expect(before[5]).toBeGreaterThan(before[3]);
      await expectStableUnderPointer(
        page,
        editor,
        async () => {
          await editor.fill(text + 'M');
          await expect(
            page.getByLabel('Source line character counts'),
          ).toBeVisible();
          expect(await geometry()).toEqual(before);
          await editor.press('Tab');
        },
        async () => {
          await expect(page.locator('#test')).toHaveAttribute(
            'data-edits',
            '2',
          );
          await expect(
            page.getByLabel('Accepted cue metrics', { exact: true }),
          ).toBeVisible();
          await expect(page.locator('#test')).toHaveAttribute(
            'data-read-revision',
            String(revision + 2),
          );
          expect(await geometry()).toEqual(before);
        },
      );
    },
  );
}
