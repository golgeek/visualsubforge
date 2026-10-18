// Actual WebView/Rust/files and native playback. File choices and DOM events
// are supplied; native-window observations and real OS choosers are separate.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, readFile, readdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
const run = promisify(execFile);
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
async function absent(path) {
  try {
    await access(path);
  } catch (e) {
    if (e.code === 'ENOENT') return true;
    throw e;
  }
  return false;
}

export async function legacyImportWorkflow({
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
      'Actual WebView/Rust/files, native playback/preview acknowledgments, imported project restore/edit/save/history/recovery. Supplied file selections and DOM input; native window observations and OS picker acceptance are separate.',
    cases: [],
    captures: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'legacy-import.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const video = join(root, 'import-video.mkv');
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
      '-f',
      'lavfi',
      '-i',
      'sine=frequency=440:sample_rate=48000:duration=10',
      '-c:v',
      'libx264',
      '-preset',
      'ultrafast',
      '-threads',
      '1',
      '-c:a',
      'pcm_s16le',
      '-shortest',
      video,
    ],
    { timeout: 30000 },
  );
  const videoBytes = await readFile(video);
  const active = join(root, 'active-import.srt');
  const activeOriginal =
    '1\n00:00:01,000 --> 00:00:03,000\nActive original 世界\n\n';
  await writeFile(active, activeOriginal);
  let browser;
  const mediaState = () =>
    invoke('media_action', { action: { kind: 'status' }, session: 0 });
  const documentIs = (label, probe) =>
    until(label, async () => {
      const d = await status();
      return probe(d) && d;
    });
  const ready = async () => {
    await browser.execute(
      () =>
        new Promise((r) =>
          requestAnimationFrame(() => requestAnimationFrame(r)),
        ),
    );
    return until('import workspace ready', () =>
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
  const input = (text) =>
    browser.execute((text) => {
      const e = document.querySelector('[aria-label="Subtitle text"]');
      e.value = text;
      e.dispatchEvent(new Event('input', { bubbles: true }));
    }, text);
  const editorText = () =>
    browser.execute(
      () => document.querySelector('[aria-label="Subtitle text"]').value,
    );
  const modalText = () =>
    browser.execute(
      () =>
        document.querySelector('dialog[aria-labelledby="legacy-import-title"]')
          ?.textContent ?? '',
    );
  const review = async () => {
    await click('Review import');
    await until('completed import inspection', () =>
      browser.execute(() => {
        const review = document.querySelector(
          '[aria-label="Legacy import review"]',
        );
        const start = [...document.querySelectorAll('button')].find(
          (b) => b.textContent.trim() === 'Review import',
        );
        return !!review && !start?.disabled;
      }),
    );
  };
  const capture = async (label) => {
    await browser.saveScreenshot(join(evidence, `${label}-webview.png`));
    const item = { label, document: await status(), media: await mediaState() };
    report.captures.push(item);
    await save();
    const labels = process.env.VSF_LEGACY_IMPORT_CAPTURE_LABELS?.split(',');
    if (
      process.env.VSF_LEGACY_IMPORT_CAPTURE === '1' &&
      (!labels || labels.includes(label))
    ) {
      await writeFile(
        join(evidence, 'capture-request.json'),
        JSON.stringify(item, null, 2),
      );
      console.log(
        `Native import capture ready: ${label}; evidence: ${evidence}`,
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
  const preview = async () => {
    const d = await status();
    return until('imported subtitle preview', async () => {
      const m = await mediaState();
      assert.equal(m.error, null);
      assert.equal(m.subtitlePreviewError, null);
      return (
        m.loaded &&
        m.paused &&
        !m.seeking &&
        m.videoOutputConfigured &&
        m.renderedFrames > 0 &&
        m.previewDocumentGeneration === d.generation &&
        m.previewDocumentRevision === d.revision &&
        m
      );
    });
  };
  const footer = () =>
    browser.$('footer [role="status"]').then((e) => e.getText());
  try {
    for (const format of ['srt', 'ass', 'ssa', 'vtt']) {
      const source = join(root, `字幕-${format}.${format}`);
      const original =
        format === 'srt'
          ? '1\n00:00:01,000 --> 00:00:03,000\n{\\an8}<i>世界</i>\n\n2\n00:00:03,100 --> 00:00:05,000\nSecond\n\n'
          : await readFile(
              resolve(`tests/fixtures/subtitles/styles.${format}`),
              'utf8',
            );
      await writeFile(source, original);
      const legacy = join(root, `legacy-${format}.vssprj`);
      const destination = join(root, `imported-${format}.vsfproject`);
      const activeProject = join(root, `active-${format}.vsfproject`);
      const copy = join(root, `import-edited-${format}.${format}`);
      const recovered = join(root, `import-recovered-${format}.${format}`);
      const foreign = format === 'srt';
      const text = `[VisualSubSync]\r\nVideoSource=${foreign ? 'Z:\\missing\\video.mkv' : '.\\import-video.mkv'}\r\nSubtitlesFile=${foreign ? 'Z:\\missing\\字幕.srt' : `.\\字幕-${format}.${format}`}\r\nFocusedTimeMs=1501\r\nWAVDisplayerPositionStartMs=1000\r\nWAVDisplayerPositionStopMs=6000\r\nWAVMode=2\r\nPeakFile=old.peak\r\nDictionary=French\r\nPresets=old.ini\r\nSubtitlesVO=reference.srt\r\nTextPipeSource=notes.txt\r\n[Plugins]\r\nHook=<img src=x onerror=alert(1)>\r\n`;
      const legacyBytes =
        format === 'ass'
          ? Buffer.from('\uFEFF' + text)
          : format === 'vtt'
            ? Buffer.concat([
                Buffer.from([0xfe, 0xff]),
                Buffer.from(text, 'utf16le').swap16(),
              ])
            : Buffer.concat([
                Buffer.from([0xff, 0xfe]),
                Buffer.from(text, 'utf16le'),
              ]);
      await writeFile(legacy, legacyBytes);
      const competing = join(root, 'competing.vsfproject');
      if (foreign) await writeFile(competing, 'An existing unrelated file');
      const choices = [
        { kind: 'media', path: video },
        { kind: 'subtitles', path: active },
        { kind: 'saveProject', path: activeProject },
        { kind: 'legacyProject', path: legacy },
        ...(foreign
          ? [
              { kind: 'media', path: video },
              { kind: 'subtitles', path: source },
              { kind: 'saveProject', path: null },
              { kind: 'saveProject', path: competing },
              {
                kind: 'saveProject',
                path: join(root, 'missing-parent', 'import.vsfproject'),
              },
              ...Array.from({ length: 3 }, () => ({
                kind: 'saveProject',
                path: destination,
              })),
            ]
          : []),
        { kind: 'saveProject', path: destination },
        ...(foreign ? [{ kind: 'project', path: destination }] : []),
        { kind: 'saveSubtitles', path: copy },
        { kind: 'project', path: destination },
      ];
      browser = await restart(choices);
      await menu('openMedia');
      await until(
        'active import media loaded',
        async () => (await mediaState()).loaded,
      );
      // Backend readiness can precede MediaSession's next status projection.
      // Native Save Project remains disabled until that projection is loaded.
      await until('frontend media readiness', () =>
        browser.$('button=Play').isEnabled(),
      );
      await ready();
      await menu('openSubtitles');
      const activeOpened = await documentIs(
        'active subtitles opened',
        (d) => d.path === active,
      );
      await ready();
      await menu('saveProject');
      await until(
        'active project saved',
        async () => !(await absent(activeProject)),
      );
      await ready();
      const attachedBytes = await readFile(activeProject);
      await input('Active accepted 世界');
      await click('Apply edit');
      const accepted = await documentIs(
        'active edit accepted',
        (d) => d.dirty && d.revision === activeOpened.revision + 1,
      );
      await ready();
      await input('Unapplied active draft');
      const before = await status();
      assert.equal(before.canUndo, true);
      const beforeMedia = await mediaState(),
        policy = (await invoke('quality_settings')).current;
      await menu('importLegacyProject');
      await until('legacy import dialog', () =>
        browser.$('dialog[aria-labelledby="legacy-import-title"]').isExisting(),
      );
      await click('Choose legacy project…');
      await until('legacy source selected', async () =>
        (await modalText()).includes(legacy),
      );
      await review();
      if (foreign) {
        assert.equal(
          await browser.$('button=Create new VSF project…').isEnabled(),
          false,
        );
        assert.match(await modalText(), /[Rr]elocat|[Cc]hoose/);
        await click('Locate media…');
        await until('old review retired', () =>
          browser
            .$('[aria-label="Legacy import review"]')
            .isExisting()
            .then((x) => !x),
        );
        await click('Locate subtitles…');
        await review();
      }
      assert.equal(
        await browser.$('button=Create new VSF project…').isEnabled(),
        true,
      );
      assert.ok((await modalText()).includes(policy.profile.name));
      assert.ok((await modalText()).includes('French'));
      assert.ok((await modalText()).includes('<img src=x onerror=alert(1)>'));
      assert.equal(await browser.$('dialog[open] img').isExisting(), false);
      await capture(`import-${format}-review`);
      if (foreign) {
        await click('Create new VSF project…');
        await until('cancelled picker retains review', () =>
          browser.$('button=Create new VSF project…').isEnabled(),
        );
        assert.equal(await absent(destination), true);
        await click('Create new VSF project…');
        await until('competing output refused', async () =>
          /already exists/.test(await modalText()),
        );
        assert.equal(
          await readFile(competing, 'utf8'),
          'An existing unrelated file',
        );
        await click('Create new VSF project…');
        await until('missing output directory refused', async () =>
          /directory|No such/.test(await modalText()),
        );
        for (const [path, bytes] of [
          [legacy, legacyBytes],
          [source, Buffer.from(original)],
          [video, videoBytes],
        ]) {
          const changed = Buffer.concat([bytes, Buffer.from('external')]);
          await writeFile(path, changed);
          await click('Create new VSF project…');
          await until('changed import input refused', async () =>
            (await modalText()).includes('A source file changed'),
          );
          assert.deepEqual(await readFile(path), changed);
          assert.equal(await absent(destination), true);
          assert.equal(
            await browser.$('button=Create new VSF project…').isEnabled(),
            false,
          );
          await writeFile(path, bytes);
          await review();
        }
      }
      await click('Create new VSF project…');
      await until('project creation receipt', () =>
        browser.$('[aria-label="Project created"]').isExisting(),
      );
      assert.deepEqual(await status(), before);
      assert.equal(await editorText(), 'Unapplied active draft');
      assert.equal((await mediaState()).session, beforeMedia.session);
      assert.deepEqual((await invoke('quality_settings')).current, policy);
      assert.deepEqual(await readFile(activeProject), attachedBytes);
      assert.equal(await readFile(active, 'utf8'), activeOriginal);
      const data = JSON.parse(await readFile(destination, 'utf8'));
      assert.equal(data.mediaPath, 'import-video.mkv');
      assert.equal(data.subtitlePath, `字幕-${format}.${format}`);
      assert.equal(data.subtitleFormat, format);
      assert.equal(data.positionSeconds, 1.501);
      assert.equal(data.options.waveformStart, 1);
      assert.equal(data.options.waveformSpan, 5);
      assert.deepEqual(data.options.quality, policy.profile);
      assert.deepEqual(data.options.timing, policy.timing);
      assert.equal(data.scenes, null);
      await click('Open created project');
      await until('import opening protects draft', () =>
        browser.$('dialog[aria-label="Unsaved subtitle changes"]').isExisting(),
      );
      if (foreign) {
        await click('Cancel');
        await ready();
        assert.deepEqual(await status(), before);
        assert.equal(await editorText(), 'Unapplied active draft');
        await click('Revert draft');
        await click('Undo edit');
        await documentIs(
          'active Undo after import',
          (d) => d.revision === accepted.revision + 1,
        );
        assert.deepEqual((await status()).cues, activeOpened.cues);
        await ready();
        await click('Redo edit');
        await documentIs(
          'active Redo after import',
          (d) => d.cues[0].text === 'Active accepted 世界',
        );
        await ready();
        await menu('openProject');
        await until('import restoration protects accepted edits', () =>
          browser
            .$('dialog[aria-label="Unsaved subtitle changes"]')
            .isExisting(),
        );
      }
      await click('Discard changes');
      const restored = await documentIs(
        'imported subtitle document restored',
        (d) => d.path === source && !d.dirty,
      );
      await ready();
      assert.equal(restored.format?.name ?? 'srt', format);
      const observed = await preview();
      assert.ok(Math.abs(observed.positionSeconds - 1.501) < 0.05);
      await capture(`import-${format}-restored`);
      const firstText = restored.cues[0].text;
      // Project seek can follow a different active cue; select the first explicitly.
      await (await browser.$(`[data-cue-id="${restored.cues[0].id}"]`)).click();
      await until(
        'first imported cue selected',
        async () => (await editorText()) === firstText,
      );
      const edited = `Imported ${firstText}`;
      await input(edited);
      await click('Apply edit');
      const changed = await documentIs(
        'imported edit accepted',
        (d) => d.dirty && d.cues[0].text === edited,
      );
      await ready();
      await click('Undo edit');
      await documentIs(
        'imported Undo',
        (d) => d.revision === changed.revision + 1,
      );
      assert.deepEqual((await status()).cues, restored.cues);
      await ready();
      await click('Redo edit');
      await documentIs('imported Redo', (d) => d.cues[0].text === edited);
      await ready();
      await menu('saveAs');
      await documentIs(
        'imported separate subtitle save',
        (d) => d.path === copy && !d.dirty,
      );
      await ready();
      const expected = original.replace(firstText, edited);
      assert.equal(await readFile(copy, 'utf8'), expected);
      assert.equal(await readFile(source, 'utf8'), original);
      // The imported project still references the original, not the Save As copy.
      await menu('openProject');
      await documentIs(
        'imported project reopens original references',
        (d) => d.path === source,
      );
      await ready();
      assert.deepEqual((await status()).cues, restored.cues);
      await input(`Recovered ${firstText}`);
      await click('Apply edit');
      const recovery = await documentIs(
        'imported recovery edit accepted',
        (d) => d.dirty && d.cues[0].text === `Recovered ${firstText}`,
      );
      await until('imported accepted checkpoint persisted', async () => {
        for (const entry of await readdir(join(root, 'recovery-v1'), {
          withFileTypes: true,
        })) {
          if (!entry.isDirectory()) continue;
          try {
            const snapshot = JSON.parse(
              await readFile(
                join(root, 'recovery-v1', entry.name, 'snapshot.json'),
                'utf8',
              ),
            );
            if (
              snapshot.source_path === source &&
              (snapshot.format ?? 'srt') === format &&
              snapshot.generation === recovery.generation &&
              snapshot.revision === recovery.revision &&
              snapshot.accepted_srt ===
                original.replace(firstText, `Recovered ${firstText}`)
            )
              return true;
          } catch (e) {
            if (e.code !== 'ENOENT') throw e;
          }
        }
        return false;
      });
      const external = original.replace(firstText, `External ${firstText}`);
      await writeFile(source, external);
      browser = await restart([
        { kind: 'saveSubtitles', path: source },
        { kind: 'saveSubtitles', path: recovered },
      ]);
      await until('matching import recovery entry', () =>
        browser.execute((source) => {
          const row = [...document.querySelectorAll('.recovery-row')].find(
            (r) => r.querySelector('span')?.title === source,
          );
          const b =
            row &&
            [...row.querySelectorAll('button')].find(
              (b) => b.textContent.trim() === 'Recover' && !b.disabled,
            );
          if (!b) return false;
          b.click();
          return true;
        }, source),
      );
      const recoveredState = await documentIs(
        'imported document recovered',
        (d) => d.requiresSaveAs && d.cues[0]?.text === `Recovered ${firstText}`,
      );
      await ready();
      assert.deepEqual(recoveredState.cues, recovery.cues);
      assert.equal(recoveredState.canUndo, false);
      await menu('saveAs');
      await until('external subtitle conflict preserved', async () =>
        /changed on disk/.test(await footer()),
      );
      await ready();
      assert.equal(await readFile(source, 'utf8'), external);
      await menu('saveAs');
      await documentIs(
        'imported recovery separate save',
        (d) => d.path === recovered && !d.dirty && !d.requiresSaveAs,
      );
      await ready();
      assert.equal(
        await readFile(recovered, 'utf8'),
        original.replace(firstText, `Recovered ${firstText}`),
      );
      assert.deepEqual(await readFile(legacy), legacyBytes);
      assert.deepEqual(await readFile(video), videoBytes);
      // This mutation was introduced by the harness to test external conflict.
      await writeFile(source, original);
      report.cases.push({
        format,
        legacySha256: hash(legacyBytes),
        sourceSha256: hash(original),
        project: data,
        retainedActiveState: true,
        originalInputsPreserved: true,
        reopenedOriginalReferences: true,
        historyAndSave: true,
        recoveryAndConflict: true,
        missingRelocatedAndStaleInputs: foreign,
        preview: observed,
      });
      await save();
      await record({
        step: `legacy ${format} project import, protected restore, save/history/recovery`,
      });
    }
    const audio = join(root, 'legacy-audio.wav');
    await run(
      process.env.VSF_FFMPEG ?? 'ffmpeg',
      [
        '-v',
        'error',
        '-nostdin',
        '-i',
        video,
        '-map',
        '0:a:0',
        '-c:a',
        'pcm_s16le',
        audio,
      ],
      { timeout: 30000 },
    );
    const audioBytes = await readFile(audio);
    const audioLegacy = join(root, 'audio-only.vssprj');
    const audioProject = join(root, 'audio-only.vsfproject');
    const audioSource =
      '[VisualSubSync]\nWAVFile=legacy-audio.wav\nWAVMode=1\nSubtitlesFile=字幕-srt.srt\nFocusedTimeMs=1250\n';
    await writeFile(audioLegacy, audioSource);
    browser = await restart([
      { kind: 'legacyProject', path: audioLegacy },
      { kind: 'saveProject', path: audioProject },
    ]);
    await menu('importLegacyProject');
    await until('audio import dialog', () =>
      browser.$('dialog[aria-labelledby="legacy-import-title"]').isExisting(),
    );
    await click('Choose legacy project…');
    await until('audio project selected', async () =>
      (await modalText()).includes(audioLegacy),
    );
    await review();
    assert.ok((await modalText()).includes('WAVFile'));
    await click('Create new VSF project…');
    await until('audio project created', () =>
      browser.$('[aria-label="Project created"]').isExisting(),
    );
    const audioData = JSON.parse(await readFile(audioProject, 'utf8'));
    assert.equal(audioData.mediaPath, 'legacy-audio.wav');
    assert.equal(audioData.positionSeconds, 1.25);
    await click('Open created project');
    await documentIs(
      'audio project subtitles restored',
      (d) => d.path === join(root, '字幕-srt.srt'),
    );
    await ready();
    const audioMedia = await until('audio-only import ready', async () => {
      const m = await mediaState();
      assert.equal(m.error, null);
      return (
        m.loaded &&
        m.paused &&
        !m.seeking &&
        m.audioTracks.length > 0 &&
        Math.abs(m.positionSeconds - 1.25) < 0.05 &&
        m
      );
    });
    const waveform = await until('raw WAV waveform decoded and painted', () =>
      browser.execute(() => {
        if (document.querySelector('.wave-message')) return null;
        const canvas = document.querySelector(
          'canvas[aria-label^="Audio waveform:"]',
        );
        if (!canvas?.width || !canvas.height) return null;
        const pixels = canvas
          .getContext('2d')
          .getImageData(0, 0, canvas.width, canvas.height).data;
        let signalPixels = 0;
        for (let i = 0; i < pixels.length; i += 4) {
          if (
            pixels[i] === 119 &&
            pixels[i + 1] === 216 &&
            pixels[i + 2] === 192 &&
            pixels[i + 3] === 255
          )
            signalPixels++;
        }
        return (
          signalPixels > 100 && {
            width: canvas.width,
            height: canvas.height,
            signalPixels,
          }
        );
      }),
    );
    await capture('import-audio-restored');
    assert.equal(await readFile(audioLegacy, 'utf8'), audioSource);
    assert.deepEqual(await readFile(audio), audioBytes);
    report.audioOnly = {
      project: audioData,
      media: audioMedia,
      waveform,
      audioSha256: hash(audioBytes),
      scope:
        'Native load/track/paused-position acknowledgment and decoded waveform painted in the actual WebView; no speaker-output acceptance.',
    };
    await record({
      step: 'legacy WAVFile fallback imports and restores audio-only media',
    });
    // Exercise cancellation through real IPC, then prove another review is admitted.
    const request = {
      path: join(root, 'legacy-ass.vssprj'),
      mediaPath: null,
      subtitlePath: null,
    };
    const id = await invoke('project_import_start', { request });
    await invoke('project_import_cancel', { id });
    const cancelled = await until(
      'native import cancellation drained',
      async () => {
        const v = await invoke('project_import_view', { id });
        return !v.running && v;
      },
    );
    assert.equal(cancelled.phase, 'cancelled');
    assert.equal(cancelled.outputPath, null);
    const next = await invoke('project_import_start', { request });
    assert.notEqual(next, id);
    const rereview = await until(
      'native import after cancellation',
      async () => {
        const v = await invoke('project_import_view', { id: next });
        return !v.running && v;
      },
    );
    assert.equal(rereview.phase, 'reviewing');
    await invoke('project_import_cancel', { id: next });
    report.nativeCancellation = true;
    report.status = 'passed';
    await save();
  } catch (e) {
    report.status = 'failed';
    report.error = String(e.stack ?? e);
    await save();
    throw e;
  }
}
