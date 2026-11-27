import { expect, test } from '@playwright/test';

for (const boundary of ['start', 'end'] as const) {
  test(`dragging the ${boundary} handle outside the view and back commits once`, async ({
    page,
  }) => {
    await page.goto('/tests/frontend/cue-timing/?manual&zoom');
    const handle = page.getByRole('button', {
      name: new RegExp(`^Cue ${boundary} boundary`),
    });
    const box = (await handle.boundingBox())!;
    const x = Math.round(box.x + box.width / 2);
    const y = Math.round(box.y + box.height / 2);
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(boundary === 'start' ? 1 : x + 50, y);
    await expect(handle).toBeFocused();
    await page.mouse.move(x + (boundary === 'start' ? 70 : -70), y);
    await page.mouse.up();
    await expect(page.getByTestId('accepted')).toContainText(
      `Accepted: ${boundary === 'start' ? '1300–4000' : '1000–3700'} ms. Commits: 1.`,
    );
  });
}
test('held arrow keeps focus and commits beyond the visible edge', async ({
  page,
}) => {
  await page.goto('/tests/frontend/cue-timing/?manual&zoom');
  const handle = page.getByRole('button', { name: /^Cue start boundary/ });
  await handle.focus();
  await page.keyboard.down('ArrowLeft');
  await expect(handle).toBeFocused();
  await page.keyboard.down('ArrowLeft');
  await page.keyboard.up('ArrowLeft');
  await expect(page.getByTestId('accepted')).toContainText(
    'Accepted: 980–4000 ms. Commits: 1.',
  );
  await expect(handle).toHaveCount(0);
});
