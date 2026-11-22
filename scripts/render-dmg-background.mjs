// Run with the repository's pinned Playwright Chromium and Inter font installed.
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const asset = (name) => new URL(`../src-tauri/dmg/${name}`, import.meta.url);
const svg = await readFile(asset('background.svg'), 'utf8');
const font = await readFile(
  new URL(
    '../node_modules/@fontsource-variable/inter/files/inter-latin-wght-normal.woff2',
    import.meta.url,
  ),
  'base64',
);
const browser = await chromium.launch();
try {
  for (const scale of [1, 2]) {
    const page = await browser.newPage({
      deviceScaleFactor: scale,
      viewport: { width: 640, height: 480 },
    });
    await page.setContent(
      `<style>@font-face {font-family:'Inter Variable';font-weight:100 900;src:url(data:font/woff2;base64,${font}) format('woff2')} body{margin:0}</style>${svg}`,
    );
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({
      path: fileURLToPath(
        asset(scale === 1 ? 'background.png' : 'background@2x.png'),
      ),
    });
    await page.close();
  }
} finally {
  await browser.close();
}
