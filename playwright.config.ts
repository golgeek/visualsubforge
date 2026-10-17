import { defineConfig, devices } from '@playwright/test';

const port = Number(process.env.VSF_BROWSER_TEST_PORT ?? 1421);
if (!Number.isInteger(port) || port < 1 || port > 65535)
  throw new Error('VSF_BROWSER_TEST_PORT must be a valid TCP port');
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: './tests/frontend',
  testMatch: '**/*.spec.ts',
  // Keep browser cleanup from deleting retained real-desktop receipts.
  outputDir: 'test-results/browser',
  workers: 1,
  retries: 0,
  forbidOnly: !!process.env.CI,
  timeout: 60_000,
  globalTimeout: 10 * 60_000,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],
  webServer: {
    command: `npm run dev -- --host 127.0.0.1 --port ${port} --strictPort`,
    url: baseURL,
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
