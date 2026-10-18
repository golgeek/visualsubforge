// Actual desktop/WebView, Rust and files. Only file selections and app-data paths
// are supplied. Optional independent native captures inspect the video sibling.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { copyFile, readFile, readdir, writeFile } from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const choice = (format) => `export${format[0].toUpperCase()}${format.slice(1)}`;

export async function conversionWorkflow({
  restart,
  samePath,
  root,
  evidence,
  menu,
  invoke,
  status,
  until,
  record,
  binary,
}) {
  const report = {
    status: 'running',
    binary,
    binarySha256: hash(await readFile(binary)),
    scope:
      'Actual WebView/Rust/files and native preview acknowledgment; generated DOM input and supplied file choices. Separate native-window observations are optional. Native picker and physical Windows/Linux acceptance are not established by this lane.',
    cases: [],
    captures: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'conversion.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const media = join(root, 'conversion-video.mkv');
  await copyFile(resolve('tests/fixtures/generated/resync/offset.mkv'), media);
  const embedded = await readFile(
    resolve('tests/fixtures/subtitles/embedded-font.ass'),
    'utf8',
  );
  const payload = embedded.slice(embedded.indexOf('[Fonts]')).trimEnd();
  const inputs = {
    srt: '7\n00:00:01,000 --> 00:00:03,000\n{\\an8\\pos(192,65)\\c&H00FF00&}<i>世界</i>\n\n9\n00:00:01,000 --> 00:00:04,000\n<b>Second</b>\nLine — été\n\n',
  };
  for (const format of ['ass', 'ssa'])
    inputs[format] =
      (
        await readFile(
          resolve(`tests/fixtures/subtitles/styles.${format}`),
          'utf8',
        )
      )
        .replace('Title,sans-serif,36', 'Title,VSF Fixture Font,56')
        .replace('Red title', '{\\pos(320,80)\\c&H00FF00&}AB') +
      '\n' +
      payload +
      '\n';
  let browser;
  const ready = () =>
    until('conversion workspace ready', () =>
      browser.execute(() => {
        const convert = [...document.querySelectorAll('button')].find(
          (b) => b.textContent.trim() === 'Convert subtitles…',
        );
        return (
          document.querySelector('main')?.getAttribute('aria-busy') ===
            'false' &&
          !document.querySelector('dialog[open]') &&
          (!document.querySelector('.cue-row') || !convert?.disabled)
        );
      }),
    );
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
  const documentIs = (label, test) =>
    until(label, async () => {
      const d = await status();
      return test(d) && d;
    });
  const dialogText = () =>
    browser.execute(
      () =>
        document.querySelector('#conversion-dialog-title')?.closest('dialog')
          ?.textContent ?? '',
    );
  const mediaState = () =>
    invoke('media_action', { action: { kind: 'status' }, session: 0 });
  async function open(target, precision = 'exact') {
    await ready();
    await menu('convertSubtitles');
    await until('conversion options', () =>
      browser.execute(
        () => !!document.querySelector('[aria-label="Conversion format"]'),
      ),
    );
    await input('Conversion format', target);
    if (precision !== 'exact') await input('Conversion precision', precision);
    await click('Review conversion');
    await until('conversion report', () =>
      browser.execute(
        () => !!document.querySelector('[aria-label="Conversion loss report"]'),
      ),
    );
  }
  async function close() {
    await click('Close conversion');
    await ready();
  }
  async function apply(text) {
    const before = await status();
    await input('Subtitle text', text);
    await click('Apply edit');
    const after = await documentIs(
      'conversion fixture edit',
      (d) => d.revision === before.revision + 1,
    );
    await ready();
    return after;
  }
  async function history(label) {
    const before = await status();
    await click(label);
    const after = await documentIs(
      label,
      (d) => d.revision === before.revision + 1,
    );
    await ready();
    return after;
  }
  async function publish(path) {
    await click('Choose new output file…');
    await until('conversion exported', () =>
      browser.execute(
        (path) =>
          document
            .querySelector('[aria-label="Conversion exported"]')
            ?.textContent.includes(path),
        path,
      ),
    );
  }
  async function preview(label) {
    const doc = await status(),
      current = await mediaState();
    await invoke('media_action', {
      action: { kind: 'seek', seconds: 1.5 },
      session: current.session,
    });
    const m = await until('converted native preview current', async () => {
      const m = await mediaState();
      assert.equal(m.subtitlePreviewError, null);
      assert.equal(m.error, null);
      return (
        m.loaded &&
        !m.seeking &&
        m.renderedFrames > 0 &&
        m.previewDocumentGeneration === doc.generation &&
        m.previewDocumentRevision === doc.revision &&
        m
      );
    });
    const capture = {
      label,
      documentGeneration: doc.generation,
      documentRevision: doc.revision,
      media: m,
    };
    report.captures.push(capture);
    await save();
    await until('native editor controls contained', () =>
      browser.execute(() => {
        const panel = document.querySelector('.editor').getBoundingClientRect();
        const bar = document
          .querySelector('.editorbar')
          .getBoundingClientRect();
        const text = document
          .querySelector('[aria-label="Subtitle text"]')
          .getBoundingClientRect();
        const footer = document
          .querySelector('.editorfooter')
          .getBoundingClientRect();
        return (
          bar.bottom <= text.top &&
          text.height >= 34 &&
          text.bottom <= footer.top &&
          footer.bottom <= panel.bottom
        );
      }),
    );
    await browser.saveScreenshot(join(evidence, `${label}-webview.png`));
    const labels = process.env.VSF_CONVERSION_CAPTURE_LABELS?.split(',');
    if (
      process.env.VSF_CONVERSION_CAPTURE === '1' &&
      (!labels || labels.includes(label))
    ) {
      await writeFile(
        join(evidence, 'capture-request.json'),
        JSON.stringify(capture, null, 2),
      );
      console.log(
        `Native conversion capture ready: ${label}; evidence: ${evidence}`,
      );
      await until(
        `native conversion capture ${label}`,
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
  }
  try {
    for (const sourceFormat of ['srt', 'ass', 'ssa'])
      for (const target of ['srt', 'ass', 'ssa']) {
        if (sourceFormat === target) continue;
        const label = `conversion-${sourceFormat}-${target}`;
        const source = join(root, `${label}.${sourceFormat}`),
          output = join(root, `${label}-export.${target}`),
          existing = join(root, `${label}-existing.${target}`),
          project = join(root, `${label}.vsfproject`);
        const original =
          '\uFEFF' + inputs[sourceFormat].replaceAll('\n', '\r\n');
        await writeFile(source, original);
        await writeFile(existing, 'External destination must survive');
        browser = await restart([
          { kind: 'media', path: media },
          { kind: 'subtitles', path: source },
          { kind: 'saveProject', path: project },
          { kind: choice(target), path: null },
          {
            kind: choice(target),
            path: join(root, 'missing-parent', `export.${target}`),
          },
          { kind: choice(target), path: existing },
          { kind: choice(target), path: output },
          { kind: 'subtitles', path: output },
          { kind: 'project', path: project },
        ]);
        await menu('openMedia');
        await until(
          'conversion media',
          async () => (await mediaState()).loaded,
        );
        await ready();
        await until('frontend media ready for project saving', () =>
          browser.execute(() =>
            [...document.querySelectorAll('button')].some(
              (b) =>
                ['Play', 'Pause'].includes(b.textContent.trim()) && !b.disabled,
            ),
          ),
        );
        await menu('openSubtitles');
        const opened = await documentIs('conversion source', (d) =>
          samePath(d.path, source),
        );
        await ready();
        await menu('saveProject');
        const projectBytes = await until(
          'conversion project saved',
          async () => {
            try {
              return await readFile(project);
            } catch (e) {
              if (e.code === 'ENOENT') return false;
              throw e;
            }
          },
        );
        await ready();
        const text = opened.cues[0].text + ' accepted';
        await apply(text);
        await apply(text + ' redo marker');
        await history('Undo edit');
        const before = await status();
        assert.ok(before.dirty && before.canUndo && before.canRedo);
        await open(target);
        const review = await dialogText();
        assert.ok(!review.includes('Export is blocked'));
        if (sourceFormat === 'srt')
          assert.ok(review.includes('Fansub ASS/SSA overrides are retained'));
        else if (target === 'srt')
          assert.ok(
            review.includes('SRT cannot store style definitions') &&
              review.includes('SRT cannot embed fonts'),
          );
        else
          assert.ok(
            review.includes('Encoded font/graphic payloads are retained'),
          );
        await browser.saveScreenshot(join(evidence, `${label}-review.png`));
        if (sourceFormat === 'srt' && target === 'ass') {
          await browser.setWindowSize(800, 650);
          await until('narrow dialog layout', () =>
            browser.execute(() => {
              const d = document
                .querySelector('#conversion-dialog-title')
                .closest('dialog');
              return d.clientWidth > 0 && d.scrollWidth <= d.clientWidth;
            }),
          );
          await browser.saveScreenshot(
            join(evidence, `${label}-review-narrow.png`),
          );
          await browser.setWindowSize(1200, 820);
        }
        await click('Choose new output file…');
        await until('cancelled picker retains review', () =>
          browser.execute(() =>
            [...document.querySelectorAll('button')].some(
              (b) =>
                b.textContent.trim() === 'Choose new output file…' &&
                !b.disabled,
            ),
          ),
        );
        assert.deepEqual(await status(), before);
        for (const failure of ['missing parent', 'existing destination']) {
          await click('Choose new output file…');
          await until(failure, () =>
            browser.execute(() => {
              const d = document
                .querySelector('#conversion-dialog-title')
                .closest('dialog');
              return (
                !!d.querySelector('[role="alert"]')?.textContent &&
                [...d.querySelectorAll('button')].some(
                  (b) =>
                    b.textContent.trim() === 'Choose new output file…' &&
                    !b.disabled,
                )
              );
            }),
          );
          assert.deepEqual(await status(), before);
        }
        assert.equal(
          await readFile(existing, 'utf8'),
          'External destination must survive',
        );
        await publish(output);
        const bytes = await readFile(output, 'utf8');
        assert.ok(
          bytes.includes(' accepted') &&
            !bytes.includes('redo marker') &&
            !bytes.startsWith('\uFEFF') &&
            !bytes.includes('\r'),
        );
        if (sourceFormat !== 'srt' && target !== 'srt')
          assert.ok(bytes.includes(payload));
        if (target === 'srt')
          assert.ok(
            bytes.includes('{\\pos(320,80)\\c&H00FF00&}AB accepted') &&
              !bytes.includes('[Fonts]'),
          );
        assert.deepEqual(await status(), before);
        assert.deepEqual(await readFile(project), projectBytes);
        assert.equal(await readFile(source, 'utf8'), original);
        await close();
        await history('Redo edit');
        assert.equal((await status()).cues[0].text, text + ' redo marker');
        await history('Undo edit');
        assert.equal((await status()).cues[0].text, text);
        let crashRecovery = false;
        if (sourceFormat === 'ass' && target === 'ssa') {
          await until(
            'conversion preserves dirty recovery checkpoint',
            async () => {
              for (const entry of await readdir(join(root, 'recovery-v1'), {
                withFileTypes: true,
              }))
                if (entry.isDirectory()) {
                  try {
                    const c = JSON.parse(
                      await readFile(
                        join(root, 'recovery-v1', entry.name, 'snapshot.json'),
                        'utf8',
                      ),
                    );
                    if (
                      c.accepted_srt?.includes(text) &&
                      !c.accepted_srt.includes('redo marker')
                    )
                      return true;
                  } catch (e) {
                    if (e.code !== 'ENOENT') throw e;
                  }
                }
              return false;
            },
          );
          const recoveredExport = join(root, 'conversion-recovered-export.ssa'),
            savedRecovery = join(root, 'conversion-recovered.ass');
          browser = await restart([
            { kind: choice(target), path: recoveredExport },
            { kind: 'saveSubtitles', path: savedRecovery },
            { kind: 'media', path: media },
            { kind: 'subtitles', path: output },
            { kind: 'project', path: project },
          ]);
          const row = await browser.$(
            `//div[contains(@class,"recovery-row")][span[starts-with(normalize-space(.),"${basename(source)} ")]]`,
          );
          await until('conversion recovery entry', () => row.isExisting());
          await (await row.$('button=Recover')).click();
          const recovered = await documentIs(
            'conversion source recovered',
            (d) => d.requiresSaveAs && d.loaded,
          );
          await ready();
          assert.equal(recovered.canUndo, false);
          assert.equal(recovered.cues[0].text, text);
          await open(target);
          await publish(recoveredExport);
          assert.deepEqual(await status(), recovered);
          assert.equal(await readFile(recoveredExport, 'utf8'), bytes);
          await close();
          await menu('saveAs');
          await documentIs(
            'recovered original saved separately',
            (d) => !d.dirty && samePath(d.path, savedRecovery),
          );
          await ready();
          await menu('openMedia');
          await until(
            'restored preview media',
            async () => (await mediaState()).loaded,
          );
          await ready();
          crashRecovery = true;
        } else {
          const restored = await history('Undo edit');
          assert.equal(restored.dirty, false);
          assert.deepEqual(restored.cues, opened.cues);
        }
        await menu('openSubtitles');
        const reopened = await documentIs('converted output reopened', (d) =>
          samePath(d.path, output),
        );
        await ready();
        assert.equal(reopened.format?.name ?? 'srt', target);
        assert.equal(reopened.cues.length, before.cues.length);
        assert.deepEqual(
          reopened.cues.map((c) => [c.startMs, c.endMs]),
          before.cues.map((c) => [c.startMs, c.endMs]),
        );
        assert.ok(reopened.cues[0].text.includes(' accepted'));
        await preview(`${label}-output`);
        await menu('openProject');
        await documentIs('original project restored', (d) =>
          samePath(d.path, source),
        );
        await ready();
        assert.equal((await status()).cues[0].text, opened.cues[0].text);
        assert.equal(await readFile(source, 'utf8'), original);
        report.cases.push({
          source: sourceFormat,
          target,
          sourceSha256: hash(original),
          outputSha256: hash(bytes),
          originalSnapshotPreserved: true,
          dirtyUndoRedoPreserved: true,
          projectPreservedAndRestored: true,
          pickerCancelled: true,
          failedExportsPreserved: true,
          exactTimesReopened: true,
          attachmentPayloadRetained: sourceFormat !== 'srt' && target !== 'srt',
          crashRecovery,
        });
        await save();
        await record({
          step: `${label} review, failures, history, output reopen and original project restoration`,
        });
      }
    const source = join(root, 'conversion-precision.srt'),
      output = join(root, 'conversion-precision.ass');
    const original =
      '1\n00:00:01,001 --> 00:00:02,000\n{\\move(20,30,80,30,0,900)}Timed\n\n';
    await writeFile(source, original);
    browser = await restart([
      { kind: 'subtitles', path: source },
      { kind: 'exportAss', path: output },
      { kind: 'subtitles', path: output },
    ]);
    await menu('openSubtitles');
    await documentIs('precision source', (d) => samePath(d.path, source));
    await ready();
    await open('ass');
    assert.ok((await dialogText()).includes('Export is blocked'));
    await input('Conversion precision', 'nearest');
    await click('Review conversion');
    await until('unsafe timed effect blocked', async () =>
      (await dialogText()).includes('Export is blocked'),
    );
    assert.match(await dialogText(), /duration|timed/i);
    await browser.saveScreenshot(
      join(evidence, 'conversion-unsafe-rounding.png'),
    );
    await close();
    await input('Cue end', '00:00:02,001');
    await click('Apply edit');
    await documentIs(
      'equal rounding shifts prepared',
      (d) => d.cues[0].endMs === 2001,
    );
    await ready();
    const accepted = await status();
    await open('ass', 'nearest');
    assert.ok(!(await dialogText()).includes('Export is blocked'));
    await publish(output);
    assert.deepEqual(await status(), accepted);
    await close();
    await history('Undo edit');
    await menu('openSubtitles');
    const rounded = await documentIs('rounded output', (d) =>
      samePath(d.path, output),
    );
    await ready();
    assert.equal(rounded.cues[0].startMs, 1000);
    assert.equal(rounded.cues[0].endMs, 2000);
    assert.equal(rounded.cues[0].text, accepted.cues[0].text);
    assert.equal(await readFile(source, 'utf8'), original);
    report.precision = {
      exactRefused: true,
      changedDurationEffectRefused: true,
      equalShiftEffectPreserved: true,
      originalPreserved: true,
      outputSha256: hash(await readFile(output)),
    };
    report.status = 'passed';
    await save();
    await record({
      step: 'explicit rounding, unsafe effect refusal and equal-shift tagged SRT export',
    });
  } catch (error) {
    report.status = 'failed';
    report.error = String(error.stack ?? error);
    await save();
    throw error;
  }
}
