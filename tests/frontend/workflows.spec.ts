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
  let release = () => {};
  let intercepted = false;
  const registration = new Promise<void>((resolve) => (release = resolve));
  await page.route(
    '**/node_modules/.vite/deps/@tauri-apps_api_event.js*',
    async (route) => {
      intercepted = true;
      await registration;
      await route.continue();
    },
  );
  try {
    await verifyChecks(page, async () => {
      await page.goto('/tests/frontend/projects/', {
        waitUntil: 'domcontentloaded',
      });
      await expect.poll(() => intercepted).toBe(true);
      // Document/menu readiness can precede the asynchronous event import.
      // Hold it until the workspace is usable, without a timing-based sleep.
      await expect(
        page.getByRole('button', { name: 'Create cue', exact: true }),
      ).toBeEnabled();
      release();
    });
  } finally {
    release();
  }
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
