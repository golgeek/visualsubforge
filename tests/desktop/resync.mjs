import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
const run = promisify(execFile);
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');

export async function styledResync({
  restart,
  root,
  evidence,
  menu,
  invoke,
  status,
  until,
  record,
  cli,
  samePath,
}) {
  const mediaRoot = join(root, 'resync-media');
  await mkdir(mediaRoot);
  for (const name of ['source.mkv', 'offset.mkv', 'speed-ads.mkv'])
    await copyFile(
      resolve('tests/fixtures/generated/resync', name),
      join(mediaRoot, name),
    );
  for (const extension of ['ass', 'ssa', 'srt']) {
    const source = join(root, `resync-source.${extension}`);
    const saved = join(root, `resync-gui.${extension}`);
    const cliOutput = join(root, `resync-cli.${extension}`);
    const original = await readFile(
      resolve(`tests/fixtures/subtitles/resync.${extension}`),
      'utf8',
    );
    await writeFile(source, original);
    const reportPath = join(root, `resync-${extension}.json`);
    await run(
      cli,
      [
        'resync',
        '--source-video',
        join(mediaRoot, 'source.mkv'),
        '--target-video',
        join(mediaRoot, 'offset.mkv'),
        '--source-subtitles',
        source,
        '--output',
        cliOutput,
        '--report',
        reportPath,
        '--json',
      ],
      { timeout: 90_000, maxBuffer: 2_000_000 },
    );
    const report = JSON.parse(await readFile(reportPath, 'utf8'));
    assert.equal(report.complete, true);
    assert.equal(report.subtitle_format, extension);
    const browser = await restart([
      { kind: 'media', path: join(mediaRoot, 'offset.mkv') },
      { kind: 'subtitles', path: source },
      { kind: 'media', path: join(mediaRoot, 'source.mkv') },
      {
        kind: 'saveSubtitles',
        path: join(root, 'missing-resync', `bad.${extension}`),
      },
      { kind: 'saveSubtitles', path: saved },
      { kind: 'media', path: join(mediaRoot, 'speed-ads.mkv') },
      { kind: 'subtitles', path: source },
      { kind: 'media', path: join(mediaRoot, 'source.mkv') },
    ]);
    async function dispatch(action) {
      await until('workspace command acknowledgment', async () =>
        browser.execute(
          () =>
            document.querySelector('main')?.getAttribute('aria-busy') ===
            'false',
        ),
      );
      await menu(action);
    }
    await dispatch('openMedia');
    await until(
      'resync target loaded',
      async () =>
        (
          await invoke('media_action', {
            action: { kind: 'status' },
            session: 0,
          })
        ).loaded,
    );
    await dispatch('openSubtitles');
    const before = await until('resync source loaded', async () => {
      const d = await status();
      return samePath(d.path, source) && d;
    });
    const click = async (label) => {
      await until(`enabled ${label}`, async () =>
        browser.execute((text) => {
          const button = [
            ...document.querySelectorAll('dialog[open] button, main button'),
          ].find((b) => b.textContent.trim() === text);
          if (!button || button.disabled) return false;
          button.click();
          return true;
        }, label),
      );
    };
    async function analyze() {
      await until('workspace ready for resync', async () =>
        browser.execute(
          () =>
            document.querySelector('main')?.getAttribute('aria-busy') ===
              'false' &&
            !document.querySelector('dialog[open]') &&
            !!document.querySelector('[data-cue-id="1"]') &&
            [...document.querySelectorAll('button')].some(
              (b) => b.textContent.trim() === 'Play' && !b.disabled,
            ),
        ),
      );
      await dispatch('resync');
      await click('Choose source video…');
      await click('Analyze videos');
      await until(
        'native resync review',
        async () =>
          browser.execute(() => !!document.querySelector('dialog[open] table')),
        90_000,
      );
    }
    await analyze();
    const review = await browser.execute(
      () => document.querySelector('dialog[open]').textContent,
    );
    assert.match(
      review,
      new RegExp(
        `${extension.toUpperCase()} endpoints round directly to the nearest\\s+${extension === 'srt' ? 1 : 10} ms tick`,
      ),
    );
    assert.match(review, /2 of\s+2 cues mapped/);
    await browser.saveScreenshot(
      join(evidence, `resync-${extension}-review.png`),
    );
    await click('Accept timings');
    const accepted = await until('native resync accepted', async () => {
      const d = await status();
      return d.requiresSaveAs && d;
    });
    assert.equal(accepted.revision, before.revision + 1);
    for (let i = 0; i < accepted.cues.length; i++) {
      assert.equal(accepted.cues[i].startMs, report.cues[i].outcome.start_ms);
      assert.equal(accepted.cues[i].endMs, report.cues[i].outcome.end_ms);
      assert.equal(accepted.cues[i].text, before.cues[i].text);
      assert.equal(accepted.cues[i].style, before.cues[i].style);
    }
    await until('resync accepted native preview', async () => {
      const m = await invoke('media_action', {
        action: { kind: 'status' },
        session: 0,
      });
      assert.equal(m.subtitlePreviewError, null);
      return (
        m.previewDocumentGeneration === accepted.generation &&
        m.previewDocumentRevision === accepted.revision
      );
    });
    await click('Undo edit');
    await until(
      'native resync Undo',
      async () => (await status()).revision === accepted.revision + 1,
    );
    assert.deepEqual((await status()).cues, before.cues);
    assert.equal((await status()).requiresSaveAs, true);
    await click('Redo edit');
    await until(
      'native resync Redo',
      async () => (await status()).revision === accepted.revision + 2,
    );
    assert.deepEqual((await status()).cues, accepted.cues);
    const beforeFailure = await status();
    await dispatch('save');
    await until('native resync failed save', async () =>
      browser.execute(
        () =>
          document.querySelector('main').getAttribute('aria-busy') ===
            'false' &&
          document
            .querySelector('main')
            .textContent.includes('Cannot access save directory'),
      ),
    );
    assert.deepEqual(await status(), beforeFailure);
    await dispatch('save');
    await until(
      'native resync saved copy',
      async () =>
        samePath((await status()).path, saved) && !(await status()).dirty,
    );
    const guiBytes = await readFile(saved),
      cliBytes = await readFile(cliOutput);
    assert.deepEqual(guiBytes, cliBytes);
    assert.equal(hash(guiBytes), report.proposed_subtitle_sha256);
    assert.equal(await readFile(source, 'utf8'), original);
    await record({
      kind: 'resync-parity',
      extension,
      outputSha256: hash(guiBytes),
      reportPath,
      gridMs: report.time_step_ms,
      sourcePreserved: true,
      undoRedo: true,
      failedSavePreserved: true,
      cliGuiExactBytes: true,
    });

    // Reopen the original for a speed change. The karaoke row is unresolved;
    // the static row maps, and no partial acceptance is possible.
    await dispatch('openMedia');
    await until(
      'native resync second target',
      async () =>
        (
          await invoke('media_action', {
            action: { kind: 'status' },
            session: 0,
          })
        ).session > 1,
    );
    await dispatch('openSubtitles');
    const unchanged = await until(
      'native resync original reopened',
      async () => {
        const d = await status();
        return samePath(d.path, source) && d;
      },
    );
    await analyze();
    const refusal = await browser.execute(() => {
      const dialog = document.querySelector('dialog[open]');
      const accept = [...dialog.querySelectorAll('button')].find(
        (b) => b.textContent.trim() === 'Accept timings',
      );
      return { text: dialog.textContent, disabled: accept.disabled };
    });
    assert.equal(refusal.disabled, true);
    assert.match(refusal.text, /Duration change affects timed tags/);
    assert.match(refusal.text, /1 of\s+2 cues mapped/);
    await browser.saveScreenshot(
      join(evidence, `resync-${extension}-refusal.png`),
    );
    await click('Close');
    assert.deepEqual(await status(), unchanged);
    assert.equal(await readFile(source, 'utf8'), original);
    await record({
      kind: 'resync-timed-refusal',
      extension,
      wholeDocumentPreserved: true,
      acceptDisabled: true,
    });
  }
}
