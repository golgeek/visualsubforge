// Scenario for the isolated release WebView harness. Both participants and every file are synthetic.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { chromium } from '@playwright/test';

export function packageChoices(root, original, copy, video, project, report) {
  const pkg = join(root, 'round1.vsfreview');
  return [
    ['subtitles', original],
    ['saveSubtitles', copy],
    ['saveReviewPackage', pkg],
    ['saveProject', project],
    ['reviewPackage', pkg],
    ['saveSubtitles', join(root, 'author.srt')],
    ['media', video],
    ['comparisonReport', report],
    ['project', project],
    ['reviewPackage', report],
  ].map(([kind, path]) => ({ kind, path }));
}
export async function exchangeRoundTrip({
  browser,
  root,
  project,
  report,
  menu,
  until,
  status,
  compared,
  record,
  invoke,
  ready,
  keyboardUndo,
}) {
  const panel = 'section[aria-label="Changes"]';
  async function press(label, scope = '') {
    await until(label, () =>
      browser.execute(
        (label, scope) => {
          const context = scope ? document.querySelector(scope) : document;
          const b = [...(context?.querySelectorAll('button') ?? [])].find(
            (b) => b.textContent.trim() === label && !b.disabled,
          );
          if (!b) return false;
          b.click();
          return true;
        },
        label,
        scope,
      ),
    );
  }
  async function fill(selector, value) {
    await until(selector, () =>
      browser.execute(
        (selector, value) => {
          const el = document.querySelector(selector);
          if (!el) return false;
          el.value = value;
          el.dispatchEvent(new Event('input', { bubbles: true }));
          return true;
        },
        selector,
        value,
      ),
    );
  }
  const editor = '[aria-label="Original"]';
  await press('Comment', editor);
  await fill(editor + ' form input', 'Ana');
  await fill(editor + ' form textarea', '<script> Check this change');
  await until('cue editor Post hit target', () =>
    browser.execute(() => {
      const post = document.querySelector(
        '[aria-label="Original"] button[type=submit]',
      );
      if (!post || post.disabled) return false;
      const r = post.getBoundingClientRect();
      return (
        document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2) ===
        post
      );
    }),
  );
  await browser.saveScreenshot(join(root, 'editor-comment.png'));
  await browser.$(editor + ' button[type=submit]').click();
  await until('posted', async () => (await compared()).discussion.open === 1);
  await until('posted comment visible', () =>
    browser.execute(
      () =>
        !!document.querySelector(
          'section[aria-label="Changes"] [aria-label="Review thread"]',
        ),
    ),
  );
  await menu('exportReviewPackage');
  await until('unsaved disclosure', () =>
    browser.execute(() =>
      document
        .querySelector('dialog[open]')
        ?.innerText.includes('The package includes your unsaved changes'),
    ),
  );
  await browser.saveScreenshot(join(root, 'reviewer-export.png'));
  await press('Export anyway', 'dialog[open]');
  const pkg = await until('round one', async () => {
    try {
      return JSON.parse(await readFile(join(root, 'round1.vsfreview'), 'utf8'));
    } catch {
      return false;
    }
  });
  assert.equal(pkg.exported_by, 'Ana');
  assert.equal(pkg.round, 1);
  assert.equal(pkg.threads.length, 1);
  await menu('saveProject');
  await until('saved project', async () => {
    try {
      return JSON.parse(await readFile(project, 'utf8')).version === 5;
    } catch {
      return false;
    }
  });
  await record(
    'Reviewer posted a literal comment and exported unsaved cues as round 1, then saved the review project',
  );
  await menu('closeSubtitles');
  await until('closed', async () => !(await status()).loaded);
  await menu('openReviewPackage');
  await until('untitled author copy', async () => {
    const s = await status();
    return s.loaded && !s.path && s.review?.round === 1;
  });
  await menu('saveAs');
  await until(
    'author saved',
    async () => (await status()).path === join(root, 'author.srt'),
  );
  await press('Later', 'dialog[open]');
  await menu('settings');
  await until('editing setting', () =>
    browser.execute(
      () =>
        !![...document.querySelectorAll('dialog[open] label')].find((l) =>
          l.textContent.includes('Your name on review comments'),
        ),
    ),
  );
  await browser.execute(() => {
    const field = [...document.querySelectorAll('dialog[open] label')]
      .find((l) => l.textContent.includes('Your name on review comments'))
      .querySelector('input');
    field.value = 'Sam';
    field.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await until(
    'author preference saved',
    async () => (await invoke('preferences_get')).reviewName === 'Sam',
  );
  await press('Close', 'dialog[open]');
  await menu('openMedia');
  await until(
    'generated media loaded',
    async () =>
      (await invoke('media_action', { action: { kind: 'status' }, session: 0 }))
        .loaded,
  );
  await until('native media controls active', () =>
    browser.execute(() =>
      [...document.querySelectorAll('button')].some(
        (b) => !b.disabled && b.textContent.trim() === 'Play',
      ),
    ),
  );
  const player = await invoke('media_action', {
    action: { kind: 'status' },
    session: 0,
  });
  await invoke('media_action', {
    action: { kind: 'pause', paused: false },
    session: player.session,
  });
  await until(
    'playing',
    async () =>
      !(
        await invoke('media_action', { action: { kind: 'status' }, session: 0 })
      ).paused,
  );
  await press('Revert', panel);
  await until(
    'thread detached',
    async () => (await compared()).discussion.detached.length === 1,
  );
  await press('Reply', panel);
  await fill(panel + ' form textarea', 'Restoring this line');
  await press('Post', panel);
  await until('author reply', () =>
    browser.execute(() =>
      document
        .querySelector('section[aria-label="Changes"]')
        ?.innerText.includes('Restoring this line'),
    ),
  );
  await press('Resolve', panel + ' [aria-label="Detached"]');
  await until('resolved', async () => (await compared()).discussion.open === 0);
  await record(
    'Author opened an untitled review, saved separately, changed the name in Settings, reverted during playback, then replied and resolved in Detached',
  );
  await until('resolved author discussion visible', () =>
    browser.execute(
      () =>
        document
          .querySelector('[aria-label="Detached"]')
          ?.innerText.includes('Reopened') ||
        document
          .querySelector('[aria-label="Detached"]')
          ?.innerText.includes('Resolved'),
    ),
  );
  await browser.execute(() =>
    document
      .querySelector('[aria-label="Detached"]')
      ?.scrollIntoView({ block: 'end' }),
  );
  await browser.saveScreenshot(join(root, 'author-detached.png'));
  await menu('exportComparisonReport');
  await press(
    (await compared()).statistics.changes
      ? 'Export anyway'
      : 'Continue to Save',
    'dialog[open]',
  );
  await until('round two', async () => (await status()).review.round === 2);
  const reader = await chromium.launch();
  try {
    const page = await reader.newPage();
    await page.goto(pathToFileURL(report).href);
    assert.equal(await page.locator('#vsf-review-package').count(), 1);
    await page.locator('label[for=f-expand]').click();
    assert(
      (await page.locator('body').innerText()).includes(
        '<script> Check this change',
      ),
    );
    await page.screenshot({ path: join(root, 'round2-browser.png') });
  } finally {
    await reader.close();
  }
  await menu('openProject');
  await press('Discard changes', 'dialog[open]');
  await until(
    'reviewer restored',
    async () => (await status()).review?.round === 1,
  );
  await menu('openReviewPackage');
  await press('Use Their Version', 'dialog[open]');
  await until('round two accepted', async () => {
    const s = await status();
    return s.review?.round === 2 && s.cues[0].text === 'one two three';
  });
  await ready();
  await browser.execute(() =>
    document.querySelector('canvas[aria-label^="Audio waveform"]')?.focus(),
  );
  // Exercise the event delivered by the macOS Edit → Undo accelerator.
  if (keyboardUndo) {
    console.log(
      'Waiting for Command-Z in the isolated reviewer window: ' + root,
    );
  } else await menu('undo');
  await until(
    'one undo restores reviewer',
    async () => (await status()).cues[0].text === 'one revised three',
    keyboardUndo ? 15 * 60_000 : 30_000,
  );
  assert.equal((await compared()).discussion.open, 0);
  if (keyboardUndo)
    await record(
      'External keyboard Undo restored the reviewer text and preserved the merged resolution',
    );
  await record(
    'Round-2 HTML opened in Chromium and VSF. Use Their Version merged the resolved discussion and one Undo restored the reviewer cues',
  );
}
