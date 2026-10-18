// Real WebView, IPC and files; queued choices and DOM input are test supplied.
// Independent native-window captures are needed to inspect the video sibling.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
const run = promisify(execFile);
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');

export async function webvttWorkflow({
  restart,
  root,
  evidence,
  menu,
  invoke,
  status,
  until,
  record,
  binary,
  cli,
}) {
  const report = {
    status: 'running',
    binary,
    binarySha256: hash(await readFile(binary)),
    scope:
      'Actual WebView/Rust/files and native preview acknowledgments. DOM input and file choices are supplied; native window observations are separate. No OS picker or Windows/Linux physical acceptance.',
    captures: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'webvtt.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const video = join(root, 'webvtt-video.mkv');
  await run(
    process.env.VSF_FFMPEG ?? 'ffmpeg',
    [
      '-v',
      'error',
      '-nostdin',
      '-f',
      'lavfi',
      '-i',
      'color=c=black:s=640x360:r=24:d=10',
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
  const source = join(root, 'source.vtt'),
    copy = join(root, 'copy.vtt'),
    recovered = join(root, 'recovered.vtt'),
    project = join(root, 'webvtt.vsfproject');
  const original =
    '\uFEFF' +
    (
      (await readFile(resolve('tests/fixtures/subtitles/styles.vtt'), 'utf8')) +
      '\nraw\n00:05.001 --> 00:07.999\n{\\an8}literal\\N <b>Bold</b> &amp; 世界\n'
    ).replaceAll('\n', '\r\n');
  await writeFile(source, original);
  report.sourceSha256 = hash(original);
  let browser;
  const mediaState = () =>
    invoke('media_action', { action: { kind: 'status' }, session: 0 });
  const documentIs = (label, test) =>
    until(label, async () => {
      const d = await status();
      return test(d) && d;
    });
  const ready = async () => {
    await browser.execute(
      () =>
        new Promise((r) =>
          requestAnimationFrame(() => requestAnimationFrame(r)),
        ),
    );
    await until('WebVTT workspace ready', () =>
      browser.execute(
        () =>
          document.querySelector('main')?.getAttribute('aria-busy') ===
            'false' && !document.querySelector('dialog[open]'),
      ),
    );
  };
  const click = (label) =>
    until(`enabled ${label}`, () =>
      browser.execute((label) => {
        const b = [...document.querySelectorAll('button')].find(
          (b) => b.textContent.trim() === label && !b.disabled,
        );
        if (!b) return false;
        b.click();
        return true;
      }, label),
    );
  const input = (label, value) =>
    browser.execute(
      (label, value) => {
        const field = document.querySelector(`[aria-label="${label}"]`);
        if (!field) throw new Error(`Missing ${label}`);
        field.value = value;
        field.dispatchEvent(
          new Event(field.tagName === 'SELECT' ? 'change' : 'input', {
            bubbles: true,
          }),
        );
      },
      label,
      value,
    );
  const message = () =>
    browser.$('footer [role="status"]').then((e) => e.getText());
  const undo = async (expected) => {
    const revision = (await status()).revision;
    await click('Undo edit');
    await documentIs('WebVTT Undo', (d) => d.revision === revision + 1);
    assert.deepEqual((await status()).cues, expected.cues);
    await ready();
  };
  const preview = async (label, seconds) => {
    const d = await status(),
      before = await mediaState();
    await invoke('media_action', {
      action: { kind: 'seek', seconds },
      session: before.session,
    });
    const media = await until('WebVTT native preview', async () => {
      const m = await mediaState();
      assert.equal(m.error, null);
      assert.equal(m.subtitlePreviewError, null);
      return (
        m.loaded &&
        !m.seeking &&
        m.videoOutputConfigured &&
        m.renderedFrames > 0 &&
        Math.abs(m.positionSeconds - seconds) < 0.05 &&
        m.previewDocumentGeneration === d.generation &&
        m.previewDocumentRevision === d.revision &&
        m
      );
    });
    await ready();
    await browser.saveScreenshot(join(evidence, `${label}-webview.png`));
    const capture = {
      label,
      seconds,
      generation: d.generation,
      revision: d.revision,
      media,
    };
    report.captures.push(capture);
    await save();
    const labels = process.env.VSF_WEBVTT_CAPTURE_LABELS?.split(',');
    if (
      process.env.VSF_WEBVTT_CAPTURE === '1' &&
      (!labels || labels.includes(label))
    ) {
      await writeFile(
        join(evidence, 'capture-request.json'),
        JSON.stringify(capture, null, 2),
      );
      console.log(
        `Native WebVTT capture ready: ${label}; evidence: ${evidence}`,
      );
      await until(
        `native observation ${label}`,
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
  try {
    browser = await restart([
      { kind: 'media', path: video },
      { kind: 'subtitles', path: source },
      { kind: 'saveSubtitles', path: null },
      {
        kind: 'saveSubtitles',
        path: join(root, 'missing-parent', 'failed.vtt'),
      },
      { kind: 'saveSubtitles', path: join(root, 'wrong.srt') },
      { kind: 'saveSubtitles', path: copy },
      { kind: 'subtitles', path: copy },
      { kind: 'saveProject', path: project },
      { kind: 'project', path: project },
    ]);
    await menu('openMedia');
    await until('WebVTT media loaded', async () => (await mediaState()).loaded);
    await ready();
    await menu('openSubtitles');
    const opened = await documentIs(
      'WebVTT opened',
      (d) => d.format?.name === 'vtt' && d.cues.length === 3,
    );
    assert.equal(opened.format.timeStepMs, 1);
    assert.equal(opened.cues[0].startMs, 1001);
    await ready();
    assert.equal(
      await browser.$('[aria-label="Cue style"]').isExisting(),
      false,
    );
    assert.equal(
      await browser.$('button=Convert subtitles…').isEnabled(),
      false,
    );
    await preview('webvtt-css-voice', 1.5);
    await preview('webvtt-timed-ruby', 4.5);
    await preview('webvtt-literal-ass', 6);

    const media = await mediaState(),
      context = (await invoke('quality_settings')).current;
    const quality = await until('WebVTT quality report', async () => {
      const result = await invoke('quality_read', {
        request: {
          session: media.session,
          generation: opened.generation,
          revision: opened.revision,
          run: true,
          restart: false,
          offset: 0,
          limit: 200,
          metricsOffset: 0,
          metricsLimit: 200,
          selected: opened.cues[0].id,
          rule: null,
          severity: null,
          draft: null,
        },
      });
      return result.phase === 'ready' && result;
    });
    const profile = join(root, 'webvtt-quality.vsfpreset');
    await invoke('quality_profile_save', {
      path: profile,
      profile: context.profile,
    });
    let cliResult;
    try {
      cliResult = await run(
        cli,
        ['check', '--subtitles', source, '--profile', profile, '--json'],
        { timeout: 20000 },
      );
    } catch (e) {
      if (![1, 3].includes(e.code)) throw e;
      cliResult = e;
    }
    const cliReport = JSON.parse(cliResult.stdout);
    assert.equal(cliReport.textInterpretation, 'webvtt_utf16_v1');
    assert.deepEqual(
      cliReport.metrics,
      quality.metrics.map((row) => row.data),
    );
    report.quality = {
      profileHash: cliReport.profileHash,
      metrics: cliReport.metrics,
      desktop: quality.metrics,
      scope:
        'Metric parity; CLI has no media and native scene coverage is reported separately.',
    };

    // Preview seeks intentionally follow the active cue; return through the
    // production list before editing the first cue, including virtualized rows.
    await browser.execute(() => {
      const list = document.querySelector('.cue-list');
      list.scrollTop = 0;
      list.dispatchEvent(new Event('scroll'));
    });
    await until('first WebVTT row visible', () =>
      browser.$('[data-cue-id="1"]').isExisting(),
    );
    await (await browser.$('[data-cue-id="1"]')).click();
    await until('first WebVTT cue selected', () =>
      browser.execute(
        (text) =>
          document.querySelector('[aria-label="Subtitle text"]').value === text,
        opened.cues[0].text,
      ),
    );
    await ready();
    await browser.execute(() => {
      const editor = document.querySelector('[aria-label="Subtitle text"]');
      const at = editor.value.indexOf('Été');
      if (at < 0) throw new Error('Missing first cue text');
      editor.focus();
      editor.setSelectionRange(at, at + 3);
    });
    await click('Bold');
    await until('WebVTT formatting draft', () =>
      browser.execute(() =>
        document
          .querySelector('[aria-label="Subtitle text"]')
          .value.includes('<b>Été</b>'),
      ),
    );
    assert.deepEqual(await status(), opened);
    await input('Cue start', '00:00:01,002');
    await click('Apply edit');
    const formatted = await documentIs(
      'WebVTT formatted edit',
      (d) => d.revision === opened.revision + 1,
    );
    assert.equal(formatted.cues[0].startMs, 1002);
    assert.equal(
      formatted.cues[0].text,
      opened.cues[0].text.replace('Été', '<b>Été</b>'),
    );
    await ready();
    await undo(opened);
    await click('Redo edit');
    await documentIs('WebVTT Redo', (d) => d.cues[0].startMs === 1002);
    assert.deepEqual((await status()).cues, formatted.cues);
    await ready();

    await click('Find / Replace…');
    await until('WebVTT text dialog', () =>
      browser.$('dialog[aria-labelledby="text-title"]').isExisting(),
    );
    await browser.execute(() => {
      const select = document.querySelector(
        'dialog[aria-labelledby="text-title"] select',
      );
      select.value = 'replace';
      select.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await until('WebVTT replacement field', () =>
      browser.$('[aria-label="Replace with"]').isExisting(),
    );
    await input('Find text', 'Été');
    await input('Replace with', 'Hiver');
    await click('Preview text changes');
    await click('Accept Replace All');
    const changed = await documentIs('WebVTT reviewed replacement', (d) =>
      d.cues[0].text.includes('<b>Hiver</b>'),
    );
    assert.equal(changed.revision, formatted.revision + 3);
    await ready();
    await undo(formatted);
    await click('Redo edit');
    await documentIs('WebVTT replacement Redo', (d) =>
      d.cues[0].text.includes('<b>Hiver</b>'),
    );
    await ready();

    const beforeTiming = await status();
    await click('Shift / align cues…');
    await input('Batch scope', 'all');
    await input('Batch offset milliseconds', '1');
    await click('Create batch preview');
    await until('WebVTT absolute marker refusal', () =>
      browser.execute(() =>
        document
          .querySelector('dialog[open]')
          ?.textContent.includes('absolute media time'),
      ),
    );
    assert.deepEqual(await status(), beforeTiming);
    await click('Close batch review');
    await ready();
    await click('Shift / align cues…');
    await input('Batch scope', 'selected');
    await input('Batch offset milliseconds', '1');
    await click('Create batch preview');
    await click('Accept batch timings');
    const shifted = await documentIs(
      'WebVTT millisecond batch',
      (d) => d.revision === beforeTiming.revision + 1,
    );
    assert.equal(shifted.cues[0].startMs, 1003);
    assert.equal(shifted.cues[0].endMs, 4000);
    assert.deepEqual(shifted.cues.slice(1), beforeTiming.cues.slice(1));
    await ready();
    await undo(beforeTiming);

    await menu('saveAs');
    await ready();
    assert.equal((await status()).dirty, true);
    await menu('saveAs');
    await until('WebVTT failed save', async () =>
      /Cannot|cannot|No such/.test(await message()),
    );
    assert.deepEqual((await status()).cues, changed.cues);
    await ready();
    await menu('saveAs');
    await until('WebVTT cross-format refusal', async () =>
      /\.vtt|format/.test(await message()),
    );
    assert.equal((await status()).path, source);
    await ready();
    await menu('saveAs');
    await documentIs('WebVTT saved', (d) => d.path === copy && !d.dirty);
    const expected = original
      .replace('00:01.001', '00:00:01.002')
      .replace('Été', '<b>Hiver</b>');
    assert.equal(await readFile(copy, 'utf8'), expected);
    assert.equal(await readFile(source, 'utf8'), original);
    await ready();
    await menu('closeSubtitles');
    await documentIs('WebVTT closed', (d) => !d.loaded);
    await ready();
    await menu('openSubtitles');
    await documentIs('WebVTT reopened', (d) => d.path === copy);
    assert.deepEqual((await status()).cues, changed.cues);
    await ready();
    await menu('saveProject');
    await until('WebVTT project saved', async () => {
      try {
        return (
          JSON.parse(await readFile(project, 'utf8')).subtitleFormat === 'vtt'
        );
      } catch (e) {
        if (e.code === 'ENOENT') return false;
        throw e;
      }
    });
    report.project = JSON.parse(await readFile(project, 'utf8'));
    assert.equal(report.project.subtitlePath, 'copy.vtt');
    await ready();
    await menu('closeSubtitles');
    await documentIs('WebVTT closed for project', (d) => !d.loaded);
    await ready();
    await menu('openProject');
    await documentIs('WebVTT project restored', (d) => d.path === copy);
    assert.deepEqual((await status()).cues, changed.cues);
    await ready();
    await preview('webvtt-project-restored', 1.5);

    const recoveryText = changed.cues[0].text.replace('Hiver', 'Recovery');
    await input('Subtitle text', recoveryText);
    await click('Apply edit');
    await documentIs(
      'WebVTT recovery edit accepted',
      (d) => d.cues[0].text === recoveryText && d.dirty,
    );
    await until('WebVTT checkpoint persisted', async () => {
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
            checkpoint.format === 'vtt' &&
            checkpoint.accepted_srt.includes('Recovery')
          )
            return true;
        } catch (e) {
          if (e.code !== 'ENOENT') throw e;
        }
      }
      return false;
    });
    const newer = expected + '\r\nNOTE external newer file\r\n';
    await writeFile(copy, newer);
    browser = await restart([
      { kind: 'saveSubtitles', path: copy },
      { kind: 'saveSubtitles', path: recovered },
    ]);
    const row = await browser.$(
      '//div[contains(@class,"recovery-row")][span[starts-with(normalize-space(.),"copy.vtt ")]]',
    );
    await until('WebVTT recovery entry', () => row.isExisting());
    await (await row.$('button=Recover')).click();
    const recovery = await documentIs(
      'WebVTT recovered',
      (d) => d.requiresSaveAs && d.cues[0]?.text === recoveryText,
    );
    assert.equal(recovery.format.name, 'vtt');
    assert.equal(recovery.canUndo, false);
    await ready();
    await menu('saveAs');
    await until('WebVTT external conflict', async () =>
      /changed on disk/.test(await message()),
    );
    assert.equal(await readFile(copy, 'utf8'), newer);
    await ready();
    await menu('saveAs');
    await documentIs(
      'WebVTT recovered copy saved',
      (d) => d.path === recovered && !d.dirty && !d.requiresSaveAs,
    );
    assert.equal(
      await readFile(recovered, 'utf8'),
      expected.replace('Hiver', 'Recovery'),
    );
    assert.equal(await readFile(source, 'utf8'), original);
    report.exactSaveReopen =
      report.undoRedo =
      report.timedRefusal =
      report.qualityParity =
        true;
    report.crashRecovery = report.externalConflictPreserved = true;
    report.status = 'passed';
    await save();
    await record({
      step: 'WebVTT actual edit/format/text/timing/quality/save/project/recovery',
      sourceSha256: report.sourceSha256,
    });
  } catch (error) {
    report.status = 'failed';
    report.error = String(error.stack ?? error);
    await save();
    throw error;
  }
}
