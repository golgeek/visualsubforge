import { expect, test } from '@playwright/test';

test('New subtitles opens one Create Project dialog and cancellation writes nothing', async ({
  page,
}) => {
  await page.goto('/tests/frontend/start-screen/?preview&createProject=srt');
  await page.getByRole('button', { name: 'New subtitles' }).click();
  await expect(page.getByLabel('Subtitle text', { exact: true })).toBeVisible();
  await page.locator('#test').dispatchEvent('fixture-save-project');
  const dialog = page.getByRole('dialog', { name: 'Create Project' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByLabel('Companion filename')).toHaveValue(
    'Interview.fr.srt',
  );
  await dialog.getByRole('button', { name: 'Cancel' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator('#test')).not.toHaveAttribute(
    'data-create-writes',
    /./,
  );
  await expect(
    page.getByText('Unsaved changes', { exact: true }),
  ).toBeVisible();
});

test('ASS creation keeps an edited companion name, reports partial publication and retries only the project', async ({
  page,
}) => {
  await page.goto('/tests/frontend/start-screen/?preview&createProject=ass');
  await page.getByRole('button', { name: 'New subtitles' }).click();
  await expect(page.getByLabel('Subtitle text', { exact: true })).toBeVisible();
  await page.locator('#test').dispatchEvent('fixture-save-project');
  const dialog = page.getByRole('dialog', { name: 'Create Project' });
  const name = dialog.getByLabel('Project name');
  const companion = dialog.getByLabel('Companion filename');
  await expect(companion).toHaveValue('Interview.fr.ass');
  await name.fill('Taken');
  await expect(companion).toHaveValue('Taken.fr.ass');
  await companion.fill('Speaker.fr.ass');
  await dialog.getByRole('button', { name: 'Choose…' }).click();
  await dialog.getByRole('button', { name: 'Create Project' }).click();
  await expect(dialog.getByRole('alert')).toContainText('Taken.vsfproject');
  await name.fill('Blocked');
  await expect(companion).toHaveValue('Speaker.fr.ass');
  await dialog.getByRole('button', { name: 'Create Project' }).click();
  await expect(dialog.getByRole('status')).toHaveText(
    'Subtitle saved to /destination/Speaker.fr.ass',
  );
  await expect(dialog.getByRole('alert')).toHaveText('Project write failed');
  await expect(page.locator('footer')).not.toContainText('Project created');
  await expect(page.getByText('Saved', { exact: true })).toBeVisible();
  await name.fill('Finished');
  await dialog.getByRole('button', { name: 'Retry project' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator('footer')).toContainText('Project created');
  const payload = JSON.parse(
    (await page.locator('#test').getAttribute('data-create-request'))!,
  );
  expect(payload.companionName).toBeNull();
  expect(payload.workspace.subtitlePath).toBe('/destination/Speaker.fr.ass');
});
