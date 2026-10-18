import { expect, test, type Page } from '@playwright/test';

// Run the same production-component harnesses that contributors can open by URL.
// Their visible results are the contract; no access to component internals or IPC.
async function verifyChecks(
  page: Page,
  load: () => Promise<unknown>,
  timeout = 45_000,
) {
  const errors: string[] = [];
  const onError = (error: Error) => errors.push(error.message);
  page.on('pageerror', onError);
  try {
    await load();
    const summary = page.getByText(/^PASS: all \d+ /);
    const failures = page.getByText(/^FAIL:/);
    await expect(summary.or(failures).first()).toBeVisible({ timeout });
    expect(await failures.allTextContents(), 'Harness failure').toEqual([]);
    expect(errors, 'Unhandled page errors').toEqual([]);
    const text = await summary.innerText();
    const count = Number(/^PASS: all (\d+) /.exec(text)?.[1]);
    expect(count, 'Harness must execute checks').toBeGreaterThan(0);
    await expect(page.locator('li').filter({ hasText: /^PASS:/ })).toHaveCount(
      count,
      { timeout },
    );
    return { summary: text, rows: await page.locator('li').allTextContents() };
  } finally {
    page.off('pageerror', onError);
  }
}

for (const name of [
  'projects',
  'recovery',
  'fast-editing',
  'new-subtitles',
  'magnetic-timing',
  'scenes',
  'cue-timing',
  'waveform-selection',
  'workspace-lifecycle',
  'resync',
  'quality-settings',
  'quality-checking',
  'gap-review',
  'scene-quality',
  'duration-quality',
  'statistics',
  'text-editing',
  'ass-documents',
  'style-authoring',
  'font-authoring',
  'conversion',
  'batch-timing',
]) {
  test(`workflow: ${name}`, async ({ page }, info) => {
    const result = await verifyChecks(page, () =>
      page.goto(`/tests/frontend/${name}/`),
    );
    await info.attach('harness-results', {
      body: [result.summary, ...result.rows].join('\n'),
      contentType: 'text/plain',
    });
    console.log(`${name}: ${result.summary}`);
  });
}

test('projects waits for delayed workspace listener registration', async ({
  page,
}) => {
  await verifyChecks(page, async () => {
    await page.goto('/tests/frontend/projects/?delayListener');
    await expect(page.locator('body')).toHaveAttribute(
      'data-workspace-listener',
      'waiting',
    );
    // Document/menu readiness can precede listener registration. Hold the
    // modeled IPC response until the workspace is usable, without a sleep.
    await expect(
      page.getByRole('button', { name: 'Create cue', exact: true }),
    ).toBeEnabled();
    await page.evaluate(() =>
      window.dispatchEvent(new Event('vsf-test-release-listener')),
    );
  });
});

test('projects remain ready after scene completion replaces the restore notice', async ({
  page,
}) => {
  await verifyChecks(page, () =>
    page.goto('/tests/frontend/projects/?observeAfterSceneCompletion'),
  );
});

// Negative controls prove a green runner requires real, complete passing rows.
test('runner rejects a FAIL row despite a passing heading', async ({
  page,
}) => {
  await expect(
    verifyChecks(page, () =>
      page.setContent('<h1>PASS: all 1 checks</h1><li>FAIL: assertion</li>'),
    ),
  ).rejects.toThrow('Harness failure');
});

test('runner rejects an uncaught page error', async ({ page }) => {
  await expect(
    verifyChecks(page, () =>
      page.setContent(
        '<script>throw new Error("control error")</script><h1>PASS: all 1 checks</h1><li>PASS: row</li>',
      ),
    ),
  ).rejects.toThrow('Unhandled page errors');
});

test('runner rejects incomplete or empty results', async ({ page }) => {
  await expect(
    verifyChecks(page, () => page.setContent('<h1>Running…</h1>'), 50),
  ).rejects.toThrow();
  await expect(
    verifyChecks(page, () => page.setContent('<h1>PASS: all 0 checks</h1>')),
  ).rejects.toThrow('Harness must execute checks');
});

test('runner rejects a heading that overcounts passing rows', async ({
  page,
}) => {
  await expect(
    verifyChecks(
      page,
      () => page.setContent('<h1>PASS: all 2 checks</h1><li>PASS: row</li>'),
      50,
    ),
  ).rejects.toThrow();
});
