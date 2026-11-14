// Run with an existing Vite server: node scripts/audit-dialogs.mjs /tmp/vsf-dialog-audit
// Captures real production components at opt-in fixture checkpoints; no CI gates change.
import { chromium } from '@playwright/test';
import { mkdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
const output = resolve(process.argv[2] || 'test-results/dialog-audit');
await mkdir(output, { recursive: true });
const browser = await chromium.launch();
const fixtures = process.argv.slice(3);
const names = fixtures.length
  ? fixtures
  : [
      'action-discovery',
      'projects',
      'plugins',
      'font-authoring',
      'style-authoring',
      'conversion',
      'legacy-import',
      'resync',
      'text-editing',
      'cue-columns',
      'cue-editor',
      'arrange-panels',
      'start-screen',
      'layout-presets',
      'window-chrome',
      'feature-modal',
    ];
const receipts = [];
try {
  for (const name of names) {
    const page = await browser.newPage({
      viewport: { width: 1100, height: 720 },
      reducedMotion: 'reduce',
    });
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    await page.goto(
      `http://localhost:1420/tests/frontend/${name}/?visual=light`,
    );
    let count = 0;
    let previous = '';
    while (true) {
      await page.waitForFunction(
        (previous) => {
          const checkpoint = document.body.dataset.visualCheckpoint;
          return (
            (checkpoint && checkpoint !== previous) ||
            /^(PASS: all|FAIL:)/.test(
              document.querySelector('#result')?.textContent || '',
            )
          );
        },
        previous,
        { timeout: 45000 },
      );
      const result = await page.locator('#result').textContent();
      if (/^(PASS: all|FAIL:)/.test(result || '')) {
        receipts.push({ fixture: name, result, errors, captures: count });
        console.log(name, result, count, 'captures');
        break;
      }
      const title = await page
        .locator('body')
        .getAttribute('data-visual-checkpoint');
      const filename = `${name}-${String(++count).padStart(2, '0')}-${title.replace(/[^a-z0-9]+/gi, '-').slice(0, 90)}.png`;
      const state = await page.evaluate(() => {
        const dialog = [...document.querySelectorAll('dialog[open]')].at(-1);
        const rect = dialog.getBoundingClientRect();
        return {
          title: document.body.dataset.visualCheckpoint,
          theme: document.documentElement.dataset.theme,
          text: dialog.innerText,
          rect: {
            x: rect.x,
            y: rect.y,
            right: rect.right,
            bottom: rect.bottom,
          },
          horizontalOverflow: dialog.scrollWidth > dialog.clientWidth + 1,
          background: getComputedStyle(dialog).backgroundColor,
          color: getComputedStyle(dialog).color,
          font: getComputedStyle(dialog).fontFamily,
        };
      });
      await page.screenshot({ path: join(output, filename) });
      receipts.push({ fixture: name, filename, ...state });
      previous = title;
      await page.evaluate(() =>
        window.dispatchEvent(new Event('vsf-visual-next')),
      );
    }
    await page.close();
  }
} finally {
  await writeFile(
    join(output, 'receipt.json'),
    JSON.stringify(receipts, null, 2),
  );
  await browser.close();
}
if (
  receipts.some(
    (receipt) => receipt.result?.startsWith('FAIL:') || receipt.errors?.length,
  )
)
  process.exitCode = 1;
