// Actual native-enabled desktop workflow. Only file choices and app-data paths
// are supplied by the harness; all edits use the production Svelte/IPC boundary.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, writeFile, readdir, copyFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
const run = promisify(execFile);

export async function assDocuments({
  menu,
  invoke,
  status,
  until,
  root,
  evidence,
  record,
  restart,
}) {
  const video = join(root, 'styled-video.mkv');
  await run(
    process.env.VSF_FFMPEG ?? 'ffmpeg',
    [
      '-v',
      'error',
      '-nostdin',
      '-f',
      'lavfi',
      '-i',
      'color=c=black:s=640x360:r=24:d=6',
      '-c:v',
      'libx264',
      '-preset',
      'ultrafast',
      '-threads',
      '1',
      video,
    ],
    { timeout: 30000 },
  );
  let browser;
  const report = {
    schema: 1,
    status: 'running',
    scope:
      'Actual Mac webview, Rust IPC, filesystem and native subtitle preview. File selections and app-data location are supplied. Generated DOM input does not prove physical input or platform installation/GPU/audio acceptance.',
    formats: [],
    captures: [],
  };
  const saveReport = () =>
    writeFile(
      join(evidence, 'ass-documents.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const state = () =>
    invoke('media_action', { action: { kind: 'status' }, session: 0 });
  const ready = async () => {
    // Let event delivery enter the frontend operation before testing its busy state.
    await browser.execute(
      () =>
        new Promise((r) =>
          requestAnimationFrame(() => requestAnimationFrame(r)),
        ),
    );
    await until('styled workspace ready', async () =>
      (await browser.$('main'))
        .getAttribute('aria-busy')
        .then((v) => v === 'false'),
    );
  };
  const input = async (label, value) => {
    await browser.execute(
      (label, value) => {
        const element = document.querySelector(`[aria-label="${label}"]`);
        if (!element) throw new Error(`Missing ${label}`);
        element.value = value;
        element.dispatchEvent(
          new Event(element.tagName === 'SELECT' ? 'change' : 'input', {
            bubbles: true,
          }),
        );
      },
      label,
      value,
    );
  };
  const click = async (name) => {
    const button = await browser.$(`button=${name}`);
    await until(`enabled ${name}`, () => button.isEnabled());
    await button.click();
  };
  const documentIs = (label, match) =>
    until(label, async () => {
      const d = await status();
      return match(d) && d;
    });
  const preview = async (label, seconds = 1.5) => {
    const doc = await status(),
      current = await state();
    await invoke('media_action', {
      action: { kind: 'seek', seconds },
      session: current.session,
    });
    const media = await until('current native subtitle preview', async () => {
      const m = await state();
      assert.equal(m.error, null);
      assert.equal(m.subtitlePreviewError, null);
      return (
        m.loaded &&
        !m.seeking &&
        m.videoOutputConfigured &&
        m.renderedFrames > 0 &&
        Math.abs(m.positionSeconds - seconds) < 0.05 &&
        m.previewDocumentGeneration === doc.generation &&
        m.previewDocumentRevision === doc.revision &&
        m
      );
    });
    await ready();
    await browser.saveScreenshot(join(evidence, `${label}-webview.png`));
    const capture = {
      label,
      seconds,
      documentGeneration: doc.generation,
      documentRevision: doc.revision,
      media,
    };
    report.captures.push(capture);
    await saveReport();
    // Optional native-window inspection: WebKit snapshots may omit sibling media
    // surfaces. The runner pauses at a settled, revision-checked frame for capture.
    const captureLabels =
      process.env.VSF_ASS_DESKTOP_CAPTURE_LABELS?.split(',');
    if (
      process.env.VSF_ASS_DESKTOP_CAPTURE === '1' &&
      (!captureLabels || captureLabels.includes(label))
    ) {
      await writeFile(
        join(evidence, 'capture-request.json'),
        JSON.stringify(capture, null, 2),
      );
      console.log(`Native capture ready: ${label}; evidence: ${evidence}`);
      await until(
        `native capture ${label}`,
        async () => {
          try {
            return (
              JSON.parse(
                await readFile(join(evidence, 'capture-ack.json'), 'utf8'),
              ).label === label
            );
          } catch (e) {
            if (e.code === 'ENOENT') return false;
            throw e;
          }
        },
        60000,
      );
    }
  };
  async function undoTo(expected) {
    await click('Undo edit');
    await documentIs(
      'complete styled Undo',
      (d) => JSON.stringify(d.cues) === JSON.stringify(expected.cues),
    );
    await ready();
  }
  try {
    for (const extension of ['ass', 'ssa']) {
      const sourcePath = join(root, `source.${extension}`),
        copy = join(root, `copy.${extension}`),
        recovered = join(root, `recovered.${extension}`),
        project = join(root, `styled-${extension}.vsfproject`);
      const original = await readFile(
        resolve(`tests/fixtures/subtitles/styles.${extension}`),
        'utf8',
      );
      await writeFile(sourcePath, original);
      browser = await restart([
        { kind: 'media', path: video },
        { kind: 'subtitles', path: sourcePath },
        { kind: 'saveSubtitles', path: null },
        {
          kind: 'saveSubtitles',
          path: join(root, 'missing-parent', `failed.${extension}`),
        },
        { kind: 'saveSubtitles', path: join(root, `wrong-${extension}.srt`) },
        { kind: 'saveSubtitles', path: copy },
        { kind: 'subtitles', path: copy },
        { kind: 'saveProject', path: project },
        { kind: 'project', path: project },
      ]);
      await menu('openMedia');
      await until('styled native media', async () => (await state()).loaded);
      await ready();
      await menu('openSubtitles');
      const opened = await documentIs(
        'opened styled document',
        (d) => d.format?.name === extension && d.cues.length === 2,
      );
      await ready();
      assert.equal(opened.format.timeStepMs, 10);
      assert.equal(opened.cues[0].style, 'Title');
      await preview(`${extension}-source`);

      await click('Inspect styles…');
      const dialog = await browser.$(
        'dialog[aria-labelledby="style-inspector-title"]',
      );
      await until('style fields visible', () => dialog.isDisplayed());
      assert.match(await dialog.getText(), /Fontsize\s*36/);
      await browser.saveScreenshot(
        join(evidence, `${extension}-style-inspector.png`),
      );
      await input('Inspect style', 'Default');
      await until('default style fields', async () =>
        /Fontsize\s*24/.test(await dialog.getText()),
      );
      assert.deepEqual(await status(), opened);
      await click('Close styles');
      await ready();
      await input('Cue style', 'Default');
      await input('Cue start', '00:00:01,001');
      await click('Apply edit');
      await until('exact precision rejected', async () =>
        /10 ms/.test(
          await (await browser.$('footer [role="status"]')).getText(),
        ),
      );
      assert.deepEqual(await status(), opened);
      assert.equal(
        await (await browser.$('[aria-label="Cue style"]')).getValue(),
        'Default',
      );
      await click('Revert draft');
      await ready();
      const text = `{\\pos(320,160)}Edited ${extension.toUpperCase()}\\N世界`;
      await input('Cue style', 'Default');
      await input('Cue start', '00:00:01,010');
      await input('Subtitle text', text.replace('\\N', '\n'));
      await click('Apply edit');
      const changed = await documentIs(
        'combined styled acceptance',
        (d) =>
          d.cues[0].text === text &&
          d.cues[0].style === 'Default' &&
          d.cues[0].startMs === 1010,
      );
      assert.equal(changed.revision, opened.revision + 1);
      assert.equal(await readFile(sourcePath, 'utf8'), original);
      await ready();
      await undoTo(opened);
      await click('Redo edit');
      await documentIs(
        'combined styled Redo',
        (d) => JSON.stringify(d.cues) === JSON.stringify(changed.cues),
      );
      await ready();
      await preview(`${extension}-edited`);

      await menu('saveAs');
      await ready();
      assert.equal((await status()).dirty, true);
      await menu('saveAs');
      await until('failed styled save reported', async () =>
        /Cannot|cannot|No such/.test(
          await (await browser.$('footer [role="status"]')).getText(),
        ),
      );
      assert.deepEqual((await status()).cues, changed.cues);
      await ready();
      await menu('saveAs');
      await until('cross-format save rejected', async () =>
        /format|extension|\.srt/.test(
          await (await browser.$('footer [role="status"]')).getText(),
        ),
      );
      assert.equal((await status()).path, sourcePath);
      assert.equal((await status()).dirty, true);
      await ready();
      await menu('saveAs');
      await documentIs('same-format save', (d) => !d.dirty && d.path === copy);
      let expected = original.replace('Red title', text);
      expected =
        extension === 'ass'
          ? expected.replace(
              'Dialogue: Title,0:00:03.00,0:00:01.00,',
              'Dialogue: Default,0:00:03.00,0:00:01.01,',
            )
          : expected.replace(
              'Dialogue: Marked=0,0:00:01.00,0:00:03.00,Title,',
              'Dialogue: Marked=0,0:00:01.01,0:00:03.00,Default,',
            );
      assert.equal(await readFile(copy, 'utf8'), expected);
      assert.equal(await readFile(sourcePath, 'utf8'), original);
      await ready();
      await menu('closeSubtitles');
      await documentIs('styled document closed', (d) => !d.loaded);
      await ready();
      await menu('openSubtitles');
      const reopened = await documentIs(
        'same-format reopened',
        (d) => d.path === copy && d.format?.name === extension,
      );
      assert.deepEqual(reopened.cues, changed.cues);
      await ready();
      await menu('saveProject');
      await until('styled project written', async () => {
        try {
          return (
            JSON.parse(await readFile(project, 'utf8')).subtitleFormat ===
            extension
          );
        } catch (e) {
          if (e.code === 'ENOENT') return false;
          throw e;
        }
      });
      const savedProject = JSON.parse(await readFile(project, 'utf8'));
      assert.equal(savedProject.subtitlePath, `copy.${extension}`);
      await ready();
      await menu('closeSubtitles');
      await documentIs('closed before project restore', (d) => !d.loaded);
      await ready();
      await menu('openProject');
      const restored = await documentIs(
        'project restored style/format',
        (d) => d.path === copy && d.format?.name === extension,
      );
      assert.deepEqual(restored.cues, changed.cues);
      await ready();
      await preview(`${extension}-project-restored`);

      const recoveryText = text.replace('Edited', 'Recovery');
      await input('Subtitle text', recoveryText);
      await click('Apply edit');
      await documentIs(
        'unsaved styled recovery edit',
        (d) => d.cues[0].text === recoveryText && d.dirty,
      );
      await until('versioned styled recovery checkpoint', async () => {
        for (const entry of await readdir(join(root, 'recovery-v1'), {
          withFileTypes: true,
        })) {
          if (!entry.isDirectory()) continue;
          try {
            const checkpoint = JSON.parse(
              await readFile(
                join(root, 'recovery-v1', entry.name, 'snapshot.json'),
                'utf8',
              ),
            );
            if (
              checkpoint.version === 4 &&
              checkpoint.accepted_srt?.includes(recoveryText)
            )
              return true;
          } catch (e) {
            if (e.code !== 'ENOENT') throw e;
          }
        }
        return false;
      });
      // The next restart is an abrupt crash of the owned process. A newer disk
      // version must survive recovery; Save As must preserve the recovered format.
      await writeFile(copy, expected + '; External newer edit\n');
      browser = await restart([
        { kind: 'saveSubtitles', path: copy },
        { kind: 'saveSubtitles', path: recovered },
      ]);
      const recoveryRow = await browser.$(
        `//div[contains(@class,"recovery-row")][span[starts-with(normalize-space(.),"copy.${extension} ")]]`,
      );
      await until('matching styled recovery entry', () =>
        recoveryRow.isExisting(),
      );
      await (await recoveryRow.$('button=Recover')).click();
      const recovery = await documentIs(
        'restored styled checkpoint',
        (d) => d.requiresSaveAs && d.cues[0]?.text === recoveryText,
      );
      assert.equal(recovery.format.name, extension);
      assert.equal(recovery.cues[0].style, 'Default');
      assert.equal(recovery.canUndo, false);
      await ready();
      await menu('saveAs');
      await until('recovered external conflict', async () =>
        /changed on disk/.test(
          await (await browser.$('footer [role="status"]')).getText(),
        ),
      );
      assert.equal(
        await readFile(copy, 'utf8'),
        expected + '; External newer edit\n',
      );
      await ready();
      await menu('saveAs');
      await documentIs(
        'separate recovered same-format copy',
        (d) => d.path === recovered && !d.dirty && !d.requiresSaveAs,
      );
      assert.equal(
        await readFile(recovered, 'utf8'),
        expected.replace(text, recoveryText),
      );
      assert.equal(await readFile(sourcePath, 'utf8'), original);
      report.formats.push({
        extension,
        sourceSha256: createHash('sha256').update(original).digest('hex'),
        sourceBytes: Buffer.byteLength(original),
        saveBytes: Buffer.byteLength(expected),
        project: savedProject,
        undoRedo: true,
        precisionRejection: true,
        failedSavePreserved: true,
        exactSaveReopen: true,
        crashRecovery: true,
        externalConflictPreserved: true,
      });
      await saveReport();
      await record({
        step: `actual ${extension.toUpperCase()} styles/edit/history/preview/save/project/recovery`,
        sourceSha256: report.formats.at(-1).sourceSha256,
      });
    }

    const tagged = join(root, 'fansub.srt'),
      taggedCopy = join(root, 'fansub-copy.srt'),
      unicode = join(root, 'available-font.ass'),
      embedded = join(root, 'embedded-font.ass'),
      embeddedCopy = join(root, 'embedded-copy.ass'),
      removed = join(root, 'removed-font.ass'),
      missing = join(root, 'missing-font.ass');
    const embeddedSource = await readFile(
      resolve('tests/fixtures/subtitles/embedded-font.ass'),
      'utf8',
    );
    await writeFile(embedded, embeddedSource);
    await writeFile(removed, embeddedSource.split('[Fonts]')[0]);
    await writeFile(
      missing,
      (await readFile(resolve('tests/fixtures/subtitles/styles.ass'), 'utf8'))
        .replaceAll('sans-serif', 'VSF Definitely Missing Font')
        .replaceAll('世界', '\u{10ffff}'),
    );
    const srt =
      '\uFEFF1\r\n00:00:01,001 --> 00:00:02,999\r\n{\\an8\\move(160,60,440,60)}<i>Fansub</i>\\N{\\c&H00FFFF&}Mixed formatting\r\n\r\n2\r\n00:00:03,010 --> 00:00:05,000\r\n{\\pos(320,180)\\fad(100,200)}<b>Timed tail</b>\r\n\r\n';
    await writeFile(tagged, srt);
    await copyFile(resolve('tests/fixtures/subtitles/styles.ass'), unicode);
    await writeFile(
      unicode,
      (await readFile(unicode, 'utf8')).replaceAll(
        'sans-serif',
        'Hiragino Sans GB',
      ),
    );
    browser = await restart([
      { kind: 'media', path: video },
      { kind: 'subtitles', path: tagged },
      { kind: 'saveSubtitles', path: taggedCopy },
      { kind: 'subtitles', path: taggedCopy },
      { kind: 'subtitles', path: unicode },
      { kind: 'subtitles', path: embedded },
      { kind: 'saveSubtitles', path: embeddedCopy },
      { kind: 'subtitles', path: removed },
      { kind: 'subtitles', path: missing },
      { kind: 'media', path: video },
    ]);
    await menu('openMedia');
    await until('tagged SRT native media', async () => (await state()).loaded);
    await ready();
    await menu('openSubtitles');
    const before = await documentIs(
      'fansub SRT opened',
      (d) => d.path === tagged,
    );
    assert.equal(before.format, undefined);
    assert.equal(before.cues[0].startMs, 1001);
    assert.equal(before.cues[0].endMs, 2999);
    await ready();
    await preview('fansub-moving-early', 1.2);
    const edited = before.cues[0].text.replace('Fansub', 'Fansub edited');
    await input('Subtitle text', edited);
    await click('Apply edit');
    const after = await documentIs(
      'fansub edited',
      (d) => d.cues[0].text === edited,
    );
    await ready();
    await undoTo(before);
    await click('Redo edit');
    await documentIs('fansub Redo', (d) => d.cues[0].text === edited);
    await ready();
    await menu('saveAs');
    await documentIs('fansub saved', (d) => d.path === taggedCopy && !d.dirty);
    assert.equal(
      await readFile(taggedCopy, 'utf8'),
      srt.replace('Fansub', 'Fansub edited'),
    );
    await ready();
    await menu('closeSubtitles');
    await documentIs('fansub closed', (d) => !d.loaded);
    await ready();
    await menu('openSubtitles');
    const reopened = await documentIs(
      'fansub reopened',
      (d) => d.path === taggedCopy,
    );
    assert.deepEqual(reopened.cues, after.cues);
    await ready();
    await preview('fansub-moving-late', 2.5);
    await preview('fansub-timed-tail', 3.5);
    await menu('openSubtitles');
    await documentIs('explicit CJK font control', (d) => d.path === unicode);
    await ready();
    await preview('ass-available-cjk-font');
    report.taggedSrt = {
      sourcePreserved: (await readFile(tagged, 'utf8')) === srt,
      millisecondEndpoints: [1001, 2999],
      exactSaveReopen: true,
      undoRedo: true,
    };
    report.explicitFontControl =
      'Hiragino Sans GB (document-defined); generic sans-serif fixture remains unchanged';
    await menu('openSubtitles');
    const attached = await documentIs(
      'embedded font opened',
      (d) => d.path === embedded,
    );
    await ready();
    await preview('ass-embedded-font');
    await input('Subtitle text', 'BA');
    await click('Apply edit');
    await documentIs('embedded font edit', (d) => d.cues[0].text === 'BA');
    await ready();
    await undoTo(attached);
    await menu('saveAs');
    await documentIs(
      'embedded font saved',
      (d) => d.path === embeddedCopy && !d.dirty,
    );
    assert.equal(await readFile(embeddedCopy, 'utf8'), embeddedSource);
    assert.equal(await readFile(embedded, 'utf8'), embeddedSource);
    await ready();
    await menu('openSubtitles');
    await documentIs('attachment removed control', (d) => d.path === removed);
    await ready();
    await preview('ass-removed-font');
    await menu('openSubtitles');
    await documentIs('missing font control', (d) => d.path === missing);
    await ready();
    await preview('ass-missing-font');
    const fontStatus = await until('native font notices', async () => {
      const m = await state();
      return (
        m.subtitleFontDiagnostics.some((text) =>
          text.includes('VSF Definitely Missing Font'),
        ) &&
        m.subtitleFontDiagnostics.some((text) =>
          text.toUpperCase().includes('10FFFF'),
        ) &&
        m
      );
    });
    await until('visible native font notices', () =>
      browser.$('[aria-label="Subtitle font notices"]').isExisting(),
    );
    assert(fontStatus.subtitleFontDiagnostics.length <= 9);
    assert.equal(fontStatus.error, null);
    assert.equal(fontStatus.subtitlePreviewError, null);
    report.fonts = {
      embeddedExactSaveAndUndo: true,
      notices: fontStatus.subtitleFontDiagnostics,
    };
    report.status = 'passed';
    await saveReport();
    await record({
      step: 'actual tagged-SRT save/history/native preview and explicit CJK font control',
    });
    await menu('closeSubtitles');
    await documentIs('final styled document closed', (d) => !d.loaded);
    await ready();
    await menu('openMedia');
    await until('font notice session reset', async () => {
      const m = await state();
      return (
        m.loaded &&
        m.session !== fontStatus.session &&
        m.subtitleFontDiagnostics.length === 0
      );
    });
    report.fonts.noticesResetWithMedia = true;
    await saveReport();
  } catch (error) {
    report.status = 'failed';
    report.error = String(error.stack ?? error);
    await saveReport();
    throw error;
  }
}
