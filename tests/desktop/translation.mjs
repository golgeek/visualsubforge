// Real WebView/Rust/files and native preview. Supplied chooser results/DOM input
// do not establish actual OS picker or physical keyboard/device acceptance.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, readFile, readdir, unlink, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
const run = promisify(execFile);
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
async function absent(path) {
  try {
    await access(path);
    return false;
  } catch (error) {
    if (error.code === 'ENOENT') return true;
    throw error;
  }
}

export async function translationWorkflow({
  restart,
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
      'Actual WebView/Rust/files, native target preview acknowledgments, four-format copy/edit/history/project/recovery and mixed-format reference attachment. File choices and DOM input are supplied; OS picker/native-window observations are separate.',
    cases: [],
    captures: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'translation.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const video = join(root, 'translation-video.mkv');
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
  let browser;
  const media = () =>
    invoke('media_action', { action: { kind: 'status' }, session: 0 });
  const documentIs = (label, probe) =>
    until(label, async () => {
      const doc = await status();
      return probe(doc) && doc;
    });
  const ready = async () => {
    await browser.execute(
      () =>
        new Promise((r) =>
          requestAnimationFrame(() => requestAnimationFrame(r)),
        ),
    );
    return until('translation workspace ready', () =>
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
        const button = [...document.querySelectorAll('button')].find(
          (b) => b.textContent.trim() === label && !b.disabled,
        );
        if (!button) return false;
        button.click();
        return true;
      }, label),
    );
  const input = (label, value) =>
    browser.execute(
      (label, value) => {
        const element = document.querySelector(`[aria-label="${label}"]`);
        if (!element) throw new Error(`Missing ${label}`);
        element.value = value;
        element.dispatchEvent(new Event('input', { bubbles: true }));
      },
      label,
      value,
    );
  const editorText = () =>
    browser.execute(
      () => document.querySelector('[aria-label="Subtitle text"]').value,
    );
  const panelText = () =>
    browser.execute(
      () =>
        document.querySelector('[aria-label="Translation reference"]')
          ?.textContent ?? '',
    );
  const footer = () =>
    browser.$('footer [role="status"]').then((e) => e.getText());
  const panel = async () => {
    await menu('translationReference');
    await until('reference panel', () =>
      browser.$('[aria-label="Translation reference"]').isExisting(),
    );
  };
  const offset = async (value) => {
    await input('Reference offset in milliseconds', String(value));
    await click('Apply offset');
    const doc = await documentIs(
      'accepted reference offset',
      (d) => d.translation?.reference?.offsetMs === value,
    );
    await ready();
    return doc;
  };
  const browse = async () => {
    await browser.execute(() => {
      const select = document.querySelector('[aria-label="Reference view"]');
      select.selectedIndex = 1;
      select.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await until('reference browse rows', () =>
      browser
        .$('[aria-label="Reference cues"] button:first-child')
        .isExisting(),
    );
    await (
      await browser.$('[aria-label="Reference cues"] button:first-child')
    ).click();
    await until('raw/reading reference view', () =>
      browser.$('[aria-label="Selected reference cue"]').isExisting(),
    );
  };
  const preview = async () => {
    const doc = await status();
    await invoke('media_action', {
      action: { kind: 'seek', seconds: doc.cues[0].startMs / 1000 + 0.1 },
      session: (await media()).session,
    });
    return until('native translation target preview', async () => {
      const m = await media();
      assert.equal(m.error, null);
      assert.equal(m.subtitlePreviewError, null);
      return (
        m.loaded &&
        m.paused &&
        !m.seeking &&
        m.videoOutputConfigured &&
        m.renderedFrames > 0 &&
        m.previewDocumentGeneration === doc.generation &&
        m.previewDocumentRevision === doc.revision &&
        m
      );
    });
  };
  const capture = async (label) => {
    await browser.saveScreenshot(join(evidence, `${label}-webview.png`));
    const item = { label, document: await status(), media: await media() };
    report.captures.push(item);
    await save();
    const labels = process.env.VSF_TRANSLATION_CAPTURE_LABELS?.split(',');
    if (
      process.env.VSF_TRANSLATION_CAPTURE === '1' &&
      (!labels || labels.includes(label))
    ) {
      await writeFile(
        join(evidence, 'capture-request.json'),
        JSON.stringify(item, null, 2),
      );
      console.log(
        `Native translation capture ready: ${label}; evidence: ${evidence}`,
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
          } catch (error) {
            if (error.code === 'ENOENT') return false;
            throw error;
          }
        },
        60000,
      );
    }
  };
  try {
    for (const format of ['srt', 'ass', 'ssa', 'vtt']) {
      const source = join(root, `原文-${format}.${format}`),
        target = join(root, `target-${format}.${format}`),
        project = join(root, `translation-${format}.vsfproject`),
        relocated = join(root, `relocated-${format}.${format}`),
        recovered = join(root, `recovered-${format}.${format}`),
        recoveredProject = join(root, `recovered-${format}.vsfproject`);
      const original =
        format === 'srt'
          ? '1\n00:00:01,000 --> 00:00:03,000\n{\\an8}<i>世界</i> <img src=x onerror=alert(1)>\n\n2\n00:00:01,000 --> 00:00:04,000\nOverlapping source\n\n'
          : await readFile(
              resolve(`tests/fixtures/subtitles/styles.${format}`),
              'utf8',
            );
      await writeFile(source, original);
      const special = format === 'srt';
      browser = await restart([
        { kind: 'media', path: video },
        { kind: 'subtitles', path: source },
        { kind: 'saveSubtitles', path: source },
        {
          kind: 'saveSubtitles',
          path: join(root, 'missing-parent', `target.${format}`),
        },
        { kind: 'saveSubtitles', path: target },
        { kind: 'saveProject', path: project },
        { kind: 'project', path: project },
        ...(special
          ? [
              { kind: 'project', path: project },
              { kind: 'project', path: project },
              { kind: 'project', path: project },
              { kind: 'subtitles', path: relocated },
              { kind: 'saveSubtitles', path: source },
            ]
          : []),
      ]);
      await menu('openMedia');
      await until(
        'native translation media',
        async () => (await media()).loaded,
      );
      await until('frontend media readiness', () =>
        browser.$('button=Play').isEnabled(),
      );
      await ready();
      await panel();
      await click('Start target copy…');
      const copied = await documentIs(
        'independent source copy',
        (d) =>
          d.loaded &&
          d.requiresSaveAs &&
          d.translation?.reference?.path === source,
      );
      await ready();
      assert.equal(copied.path, null);
      assert.equal(copied.format?.name ?? 'srt', format);
      assert.equal(copied.translation.reference.sha256, hash(original));
      assert.ok(copied.cues.length >= 2);
      const first = copied.cues[0],
        firstText = first.text;
      await menu('saveAs');
      await until('reference overwrite refused', async () =>
        /reference|source|original/i.test(await footer()),
      );
      await ready();
      assert.equal(await readFile(source, 'utf8'), original);
      await menu('saveAs');
      await until('invalid target destination refused', async () =>
        /directory|No such|cannot/i.test(await footer()),
      );
      await ready();
      assert.equal((await status()).requiresSaveAs, true);
      await menu('saveAs');
      await documentIs(
        'separate target saved',
        (d) => d.path === target && !d.requiresSaveAs,
      );
      await ready();
      assert.equal(await readFile(target, 'utf8'), original);
      const translatedText = `Translated ${firstText}`;
      await input('Subtitle text', translatedText);
      const beforeOffset = await status();
      await offset(-1500);
      assert.equal(await editorText(), translatedText);
      assert.deepEqual((await status()).cues, beforeOffset.cues);
      assert.equal((await status()).revision, beforeOffset.revision);
      await browse();
      assert.equal(
        await browser
          .$('[aria-label="Selected reference cue"] img')
          .isExisting(),
        false,
      );
      assert.ok(
        (
          await browser.$('[aria-label="Selected reference cue"]').getText()
        ).includes(firstText),
      );
      assert.equal(await editorText(), translatedText);
      await click('Apply edit');
      const edited = await documentIs(
        'translation edit accepted',
        (d) => d.cues[0].text === translatedText && d.canUndo,
      );
      await ready();
      await click('Undo edit');
      await documentIs(
        'translation Undo',
        (d) => d.revision === edited.revision + 1,
      );
      await ready();
      assert.deepEqual((await status()).cues, copied.cues);
      assert.equal((await status()).translation.reference.offsetMs, -1500);
      await click('Redo edit');
      await documentIs(
        'translation Redo',
        (d) => d.cues[0].text === translatedText,
      );
      await ready();
      await menu('save');
      await documentIs('translated target saved', (d) => !d.dirty);
      await ready();
      const translated = original.replace(firstText, translatedText);
      assert.equal(await readFile(target, 'utf8'), translated);
      assert.equal(await readFile(source, 'utf8'), original);
      await menu('saveProject');
      await until(
        'translation project saved',
        async () => !(await absent(project)),
      );
      await ready();
      const data = JSON.parse(await readFile(project, 'utf8'));
      assert.equal(data.version, 2);
      assert.equal(data.translation.reference.sha256, hash(original));
      assert.equal(data.translation.reference.offsetMs, -1500);
      assert.equal(
        data.translation.copiedFrom.path,
        `原文-${format}.${format}`,
      );
      assert.equal((await status()).translation.dirty, false);
      const generation = (await status()).generation;
      await menu('openProject');
      await documentIs(
        'translation project restored',
        (d) =>
          d.generation > generation &&
          d.translation?.reference?.sha256 === hash(original),
      );
      await ready();
      assert.equal((await status()).cues[0].text, translatedText);
      assert.equal((await status()).translation.reference.offsetMs, -1500);
      const observed = await preview();
      await browse();
      await capture(`translation-${format}-restored`);
      let capturedReference = original,
        referencePath = source;
      if (special) {
        const changed = original.replace(
          firstText,
          `Changed reference ${firstText}`,
        );
        await writeFile(source, changed);
        await input(
          'Subtitle text',
          'Protect draft across reference preflight',
        );
        const before = await status(),
          session = (await media()).session;
        await menu('openProject');
        await until('changed reference prompt', () =>
          browser
            .$('dialog[aria-labelledby="project-reference-title"][open]')
            .isExisting(),
        );
        await click('Cancel Open');
        await ready();
        assert.deepEqual(await status(), before);
        assert.equal(
          await editorText(),
          'Protect draft across reference preflight',
        );
        assert.equal((await media()).session, session);
        await click('Revert draft');
        await menu('openProject');
        await until('explicit reference reload', () =>
          browser.$('button=Reload Changed Reference').isExisting(),
        );
        await click('Reload Changed Reference');
        await documentIs(
          'changed reference adopted explicitly',
          (d) => d.translation?.reference?.sha256 === hash(changed),
        );
        await ready();
        assert.equal((await status()).translation.dirty, true);
        await menu('saveProject');
        await documentIs(
          'reloaded identity persisted',
          (d) => !d.translation.dirty,
        );
        await ready();
        await writeFile(relocated, changed);
        await unlink(source);
        await menu('openProject');
        await until('missing reference prompt', () =>
          browser
            .$('dialog[aria-labelledby="project-reference-title"][open]')
            .isExisting(),
        );
        await click('Choose Reference…');
        await documentIs(
          'missing reference relocated explicitly',
          (d) => d.translation?.reference?.path === relocated,
        );
        await ready();
        assert.equal((await status()).translation.dirty, true);
        await menu('saveProject');
        await documentIs('relocation persisted', (d) => !d.translation.dirty);
        await ready();
        await menu('saveAs');
        await until('deleted copied origin protected', async () =>
          /reference|source|original/i.test(await footer()),
        );
        await ready();
        assert.equal(await absent(source), true);
        await writeFile(source, original);
        capturedReference = changed;
        referencePath = relocated;
      }
      // Recovery must keep an exact captured reference even if it disappears.
      const current = await status();
      await (await browser.$(`[data-cue-id="${current.cues[0].id}"]`)).click();
      await until(
        'first target cue selected',
        async () => (await editorText()) === translatedText,
      );
      const recoveryText = `Recovered ${firstText}`;
      await input('Subtitle text', recoveryText);
      await click('Apply edit');
      await documentIs(
        'recovery target edit',
        (d) => d.cues[0].text === recoveryText,
      );
      await ready();
      const recovery = await offset(-3000);
      await until('reference-aware recovery checkpoint', async () => {
        for (const entry of await readdir(join(root, 'recovery-v1'), {
          withFileTypes: true,
        })) {
          if (!entry.isDirectory()) continue;
          try {
            const stored = JSON.parse(
              await readFile(
                join(root, 'recovery-v1', entry.name, 'snapshot.json'),
                'utf8',
              ),
            );
            if (
              stored.source_path === target &&
              stored.generation === recovery.generation &&
              stored.revision === recovery.revision &&
              stored.accepted_srt ===
                original.replace(firstText, recoveryText) &&
              stored.translation?.reference?.offset_ms === -3000 &&
              stored.translation.reference.text === capturedReference
            )
              return true;
          } catch (error) {
            if (error.code !== 'ENOENT') throw error;
          }
        }
        return false;
      });
      await unlink(referencePath);
      const external = original.replace(
        firstText,
        `External target ${firstText}`,
      );
      await writeFile(target, external);
      browser = await restart([
        { kind: 'saveSubtitles', path: source },
        { kind: 'saveSubtitles', path: target },
        { kind: 'saveSubtitles', path: recovered },
        { kind: 'saveProject', path: recoveredProject },
      ]);
      await until('recover target with missing reference', () =>
        browser.execute((path) => {
          const row = [...document.querySelectorAll('.recovery-row')].find(
            (r) => r.querySelector('span')?.title === path,
          );
          const button =
            row &&
            [...row.querySelectorAll('button')].find(
              (b) => b.textContent.trim() === 'Recover' && !b.disabled,
            );
          if (!button) return false;
          button.click();
          return true;
        }, target),
      );
      const restored = await documentIs(
        'captured translation recovered',
        (d) =>
          d.requiresSaveAs &&
          d.cues[0]?.text === recoveryText &&
          d.translation?.reference?.sha256 === hash(capturedReference),
      );
      await ready();
      assert.equal(restored.canUndo, false);
      assert.deepEqual(restored.cues, recovery.cues);
      assert.equal(restored.translation.reference.offsetMs, -3000);
      assert.equal(await absent(referencePath), true);
      await panel();
      await browse();
      assert.ok((await panelText()).includes('Reading text'));
      await menu('saveAs');
      await until('recovered copy origin protected', async () =>
        /reference|source|original/i.test(await footer()),
      );
      await ready();
      await menu('saveAs');
      await until('recovered external target preserved', async () =>
        /changed on disk/i.test(await footer()),
      );
      await ready();
      assert.equal(await readFile(target, 'utf8'), external);
      await menu('saveAs');
      await documentIs(
        'recovered separate target saved',
        (d) => d.path === recovered && !d.requiresSaveAs && !d.dirty,
      );
      await ready();
      assert.equal(
        await readFile(recovered, 'utf8'),
        original.replace(firstText, recoveryText),
      );
      await writeFile(referencePath, capturedReference);
      await menu('saveProject');
      await until(
        'subtitle-only recovered project saved',
        async () => !(await absent(recoveredProject)),
      );
      await ready();
      const recoveredData = JSON.parse(
        await readFile(recoveredProject, 'utf8'),
      );
      assert.equal(recoveredData.mediaPath, null);
      assert.equal(
        recoveredData.translation.reference.sha256,
        hash(capturedReference),
      );
      assert.equal((await status()).translation.dirty, false);
      await writeFile(source, original);
      report.cases.push({
        format,
        sourceSha256: hash(original),
        copiedWithoutLoss: true,
        historyAndDraftPreserved: true,
        protectedSourceAndFailedSave: true,
        project: data,
        recoveredProject: recoveredData,
        changedAndRelocatedReference: special,
        missingReferenceRecovery: true,
        preview: observed,
      });
      await save();
      await record({
        step: `translation ${format} copy/history/project/source protection/recovery`,
      });
    }
    // Existing ASS target and a WebVTT reference remain distinct formats and histories.
    const mixed = join(root, 'mixed-target.ass'),
      mixedProject = join(root, 'mixed.vsfproject');
    const ass = await readFile(join(root, '原文-ass.ass'), 'utf8'),
      reference = join(root, '原文-vtt.vtt');
    await writeFile(mixed, ass);
    browser = await restart([
      { kind: 'subtitles', path: mixed },
      { kind: 'subtitles', path: reference },
      { kind: 'saveProject', path: mixedProject },
    ]);
    await menu('openSubtitles');
    const before = await documentIs(
      'mixed target opened',
      (d) => d.path === mixed,
    );
    await ready();
    await panel();
    await input('Subtitle text', 'Unapplied mixed-format translation');
    await click('Attach reference…');
    await documentIs(
      'mixed reference attached',
      (d) => d.translation?.reference?.format === 'vtt',
    );
    await ready();
    assert.equal(await editorText(), 'Unapplied mixed-format translation');
    assert.deepEqual((await status()).cues, before.cues);
    assert.deepEqual((await status()).format, before.format);
    assert.equal((await status()).revision, before.revision);
    await click('Revert draft');
    await menu('saveProject');
    await until(
      'mixed project saved',
      async () => !(await absent(mixedProject)),
    );
    await ready();
    assert.equal(await readFile(mixed, 'utf8'), ass);
    await browse();
    await capture('translation-mixed-reference');
    report.mixed = {
      target: 'ass',
      reference: 'vtt',
      preservedDraftAndFormat: true,
      project: JSON.parse(await readFile(mixedProject, 'utf8')),
    };
    report.status = 'passed';
    await save();
    await record({
      step: 'mixed ASS target/WebVTT reference and subtitle-only project',
    });
  } catch (error) {
    report.status = 'failed';
    report.error = String(error.stack ?? error);
    await save();
    throw error;
  }
}
