import { expect, test } from '@playwright/test';

for (const [route, question, choices] of [
  ['open', 'Open /other.srt?', ['Use in project', 'Open without project']],
  [
    'startReview',
    'Start review in /captions.reviewed.srt?',
    ['Use in project', 'Open without project'],
  ],
  [
    'compareFiles',
    'as the revised subtitles',
    ['Use in project', 'Open without project'],
  ],
  [
    'recover',
    'recovered subtitles',
    ['Use in project', 'Open without project'],
  ],
  [
    'extractOwn',
    'extracted subtitles',
    ['Use in project', 'Open without project'],
  ],
  ['extractOther', 'Opening /other.mkv', ['Open without project']],
  ['recentOwn', 'Open /other.srt?', ['Use in project', 'Open without project']],
  ['recentOther', 'Opening /other.mkv', ['Open without project']],
  ['dropOwn', 'Open /other.srt?', ['Use in project', 'Open without project']],
  ['dropOther', 'Opening /other.mkv', ['Open without project']],
  [
    'close',
    'Close the working subtitles?',
    ['Use in project', 'Close without project'],
  ],
  ['closeOnly', 'only subtitle', ['Close without project']],
] as const) {
  test(`project subtitle prompt for ${route}`, async ({ page }) => {
    await page.goto(`/tests/frontend/projects/?referenceTrial=${route}`);
    const dialog = page.getByRole('dialog', { name: 'Project subtitles' });
    await expect(dialog).toContainText(question);
    await expect(dialog.getByRole('button')).toHaveText(['Cancel', ...choices]);
    await expect(dialog.getByRole('button', { name: 'Cancel' })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await expect(page.locator('body')).toHaveAttribute(
      'data-working-subtitle',
      route.startsWith('recent') ? '' : '/captions.srt',
    );
    await expect(page.locator('body')).not.toHaveAttribute(
      'data-detached',
      'yes',
    );
  });
}

for (const [route, choice] of [
  ['recentOther', 'Open without project'],
  ['recentOwn', 'Use in project'],
  ['dropOther', 'Open without project'],
  ['dropOwn', 'Use in project'],
  ['extractOther', 'Open without project'],
  ['extractOwn', 'Use in project'],
  ['closeOnly', 'Close without project'],
  ['save', 'Update project'],
  ['save', 'Cancel'],
  ['quit', 'Update project'],
  ['quit', 'Cancel'],
  ['quit', 'Quit without updating the project'],
] as const) {
  test(`${choice} on ${route} preserves its project and media decision`, async ({
    page,
  }) => {
    await page.goto(`/tests/frontend/projects/?referenceTrial=${route}`);
    const dialog = page.getByRole('dialog', { name: 'Project subtitles' });
    if (route === 'save' || route === 'quit') {
      await expect(dialog).toContainText('/captions.srt');
      await expect(dialog).toContainText('/other.srt');
      await expect(dialog.getByRole('button')).toHaveText(
        route === 'save'
          ? ['Cancel', 'Update project']
          : ['Cancel', 'Update project', 'Quit without updating the project'],
      );
    }
    await dialog.getByRole('button', { name: choice, exact: true }).click();
    await expect(dialog).toBeHidden();
    const state = page.locator('body');
    if (choice.includes('without project'))
      await expect(state).toHaveAttribute('data-detached', 'yes');
    else await expect(state).not.toHaveAttribute('data-detached', 'yes');
    if (
      route.startsWith('recent') ||
      route.startsWith('drop') ||
      route.startsWith('extract')
    ) {
      if (route.startsWith('drop') || route.startsWith('recent'))
        await expect(state).toHaveAttribute(
          'data-working-subtitle',
          '/other.srt',
        );
      else
        await expect(state).toHaveAttribute('data-accepted-extraction', 'yes');
      await expect(state).toHaveAttribute(
        'data-open-media',
        route.endsWith('Own') ? '/clip.mkv' : '/other.mkv',
      );
    }
    if (route === 'closeOnly')
      await expect(state).toHaveAttribute('data-working-subtitle', '');
    if (choice === 'Update project')
      await expect(state).toHaveAttribute('data-saved-subtitle', '/other.srt');
    else
      await expect(state).not.toHaveAttribute(
        'data-saved-subtitle',
        '/other.srt',
      );
    if (route === 'quit' && choice !== 'Cancel')
      await expect(state).toHaveAttribute('data-exited', 'yes');
    else await expect(state).not.toHaveAttribute('data-exited', 'yes');
  });
}

test('Discard on quit after subtitle Save As preserves the project reference', async ({
  page,
}) => {
  await page.goto('/tests/frontend/projects/?referenceTrial=discardQuit');
  await page
    .getByRole('button', { name: 'Discard changes', exact: true })
    .click();
  await expect(page.locator('body')).toHaveAttribute('data-exited', 'yes');
  await expect(page.locator('body')).not.toHaveAttribute(
    'data-saved-subtitle',
    '/other.srt',
  );
});
