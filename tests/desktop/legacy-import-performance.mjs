// Exact library fixtures through real WebView/Rust/files and native project open.
// DOM events and chooser responses are supplied; physical input is not measured.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';
const run = promisify(execFile);
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const fixtures = [
  [
    'plain',
    'vtt',
    'webvtt/10000-plain.vtt',
    '337cffd83d4ad10fc053bd565f96b4cbc77fd7c817db39fc40e6d254a5d09198',
  ],
  [
    'styled',
    'vtt',
    'webvtt/10000-styled.vtt',
    '783a7ad89406073e0cb6512c9ca9daaac9b7c09baeaa933dada2bd0e67aa339f',
  ],
  [
    'timed',
    'vtt',
    'webvtt/10000-timed.vtt',
    '46430c8e7b374e2bd6ccb3d4bd95cf3489d2571f00ff8c9120c9c59ce5e1c1e6',
  ],
  [
    'ass',
    'ass',
    'ass-ssa/10000.ass',
    '00e4deb1210c47e2943c30744da0e5d0512a57ba159b659d9dd16ef5d47c1c0c',
  ],
];

async function install(browser) {
  await browser.execute(() => {
    const invoke = window.__TAURI__.core.invoke;
    const b = {
      invoke,
      state: () =>
        invoke('document_action', {
          action: { kind: 'status' },
          generation: 0,
          revision: 0,
        }),
      menu: (action) => window.__TAURI__.event.emit('workspace-action', action),
      frame: () => new Promise((r) => requestAnimationFrame(r)),
      ready: () =>
        document.querySelector('main')?.getAttribute('aria-busy') === 'false' &&
        !document.querySelector('dialog[open]'),
      button: (label) =>
        [...document.querySelectorAll('button')].find(
          (b) => b.textContent.trim() === label && !b.disabled,
        ),
      input: (text) => {
        const field = document.querySelector('[aria-label="Subtitle text"]');
        field.value = text;
        field.dispatchEvent(new Event('input', { bubbles: true }));
      },
      wait: async (probe) => {
        const end = performance.now() + 8000;
        while (performance.now() < end) {
          const value = await probe();
          if (value) return value;
          await new Promise((r) => setTimeout(r, 10));
        }
        throw new Error('Legacy import desktop measurement did not settle');
      },
    };
    b.click = async (label) => (await b.wait(() => b.button(label))).click();
    b.history = async (label, revision) => {
      const at = performance.now();
      await b.click(label);
      await b.wait(
        async () => (await b.state()).revision === revision + 1 && b.ready(),
      );
      await b.frame();
      return performance.now() - at;
    };
    window.__legacyBench = b;
  });
}

export async function legacyImportPerformance({
  restart,
  status,
  until,
  root,
  evidence,
  binary,
  record,
}) {
  const report = {
    schema: 1,
    status: 'running',
    budgetMs: 1000,
    preparationDeadlineMs: 10000,
    binary,
    binarySha256: hash(await readFile(binary)),
    scope:
      'Actual WebView/Rust/files/native project restoration; five review/publication cycles per exact 10k input, full reopen/history/save/recovery, cancellation and stale-input refusal. Supplied file choices and DOM input. Pure CPU/RSS and physical native observation are separate evidence.',
    workloads: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'legacy-import-performance.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const media = join(root, 'import-scale.mkv');
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
      media,
    ],
    { timeout: 30000 },
  );
  const mediaHash = hash(await readFile(media));
  const active = join(root, 'import-scale-active.srt');
  const activeBytes =
    '1\n00:00:01,000 --> 00:00:03,000\nActive source 世界\n\n';
  await writeFile(active, activeBytes);
  try {
    for (const [kind, format, fixture, digest] of fixtures) {
      const bytes = await readFile(
        resolve('tests/fixtures/generated', fixture),
      );
      assert.equal(hash(bytes), digest);
      const original = bytes.toString('utf8');
      const source = join(root, `字幕-${kind}.${format}`);
      const legacy = join(root, `legacy-scale-${kind}.vssprj`);
      const copy = join(root, `${kind}-copy.${format}`);
      const recovered = join(root, `${kind}-recovered.${format}`);
      const outputs = Array.from({ length: 5 }, (_, i) =>
        join(root, `${kind}-${i}.vsfproject`),
      );
      const legacyBytes = Buffer.concat([
        Buffer.from([0xff, 0xfe]),
        Buffer.from(
          `[VisualSubSync]\r\nVideoSource=.\\import-scale.mkv\r\nSubtitlesFile=.\\字幕-${kind}.${format}\r\nFocusedTimeMs=1501\r\nWAVDisplayerPositionStartMs=1000\r\nWAVDisplayerPositionStopMs=6000\r\nDictionary=French\r\n`,
          'utf16le',
        ),
      ]);
      await writeFile(source, bytes);
      await writeFile(legacy, legacyBytes);
      let browser = await restart([
        { kind: 'subtitles', path: active },
        ...outputs.flatMap((path) => [
          { kind: 'legacyProject', path: legacy },
          { kind: 'saveProject', path },
        ]),
        { kind: 'saveSubtitles', path: copy },
        { kind: 'project', path: outputs.at(-1) },
      ]);
      await install(browser);
      const result = {
        kind,
        format,
        sourceSha256: digest,
        projectSha256: hash(legacyBytes),
        mediaSha256: mediaHash,
        inputBytes: bytes.length,
        cycles: [],
        timings: {},
      };
      report.workloads.push(result);
      await save();
      await browser.execute(async (active) => {
        const b = window.__legacyBench;
        await b.menu('openSubtitles');
        await b.wait(
          async () => (await b.state()).path === active && b.ready(),
        );
      }, active);
      const activeState = await status();
      for (let cycle = 0; cycle < 5; cycle++) {
        const times = await browser.execute(
          async (legacy, output, last) => {
            const b = window.__legacyBench,
              before = await b.state();
            let at = performance.now();
            await b.menu('importLegacyProject');
            await b.wait(() => b.button('Choose legacy project…'));
            await b.frame();
            const dialogMs = performance.now() - at;
            await b.click('Choose legacy project…');
            await b.wait(() =>
              document
                .querySelector('dialog[open]')
                ?.textContent.includes(legacy),
            );
            at = performance.now();
            await b.click('Review import');
            await b.frame();
            const reviewStartMs = performance.now() - at;
            const review = await b.wait(
              () =>
                b.button('Create new VSF project…') &&
                document.querySelector('[aria-label="Legacy import review"]'),
            );
            await b.frame();
            const preparationMs = performance.now() - at;
            if (
              !review.textContent
                .replace(/\s+/g, ' ')
                .includes(
                  `${(10000).toLocaleString()} cues`.replace(/\s+/g, ' '),
                )
            )
              throw new Error('Import review lost cues');
            if (review.querySelectorAll('li').length > 16)
              throw new Error('Unbounded import report');
            at = performance.now();
            await b.click('Create new VSF project…');
            await b.wait(() =>
              document
                .querySelector('[aria-label="Project created"]')
                ?.textContent.includes(output),
            );
            await b.frame();
            const publicationMs = performance.now() - at;
            if (JSON.stringify(await b.state()) !== JSON.stringify(before))
              throw new Error('Import publication changed accepted document');
            let closeMs = null;
            if (!last) {
              at = performance.now();
              await b.click('Close import');
              await b.wait(b.ready);
              await b.frame();
              closeMs = performance.now() - at;
            }
            return {
              dialogMs,
              reviewStartMs,
              preparationMs,
              publicationMs,
              closeMs,
            };
          },
          legacy,
          outputs[cycle],
          cycle === 4,
        );
        result.cycles.push(times);
        assert.deepEqual(await status(), activeState);
        const data = JSON.parse(await readFile(outputs[cycle], 'utf8'));
        assert.equal(data.mediaPath, 'import-scale.mkv');
        assert.equal(data.subtitlePath, `字幕-${kind}.${format}`);
        assert.equal(data.subtitleFormat, format);
        assert.equal(data.positionSeconds, 1.501);
        assert.equal(data.options.waveformStart, 1);
        assert.equal(data.options.waveformSpan, 5);
        assert.equal(data.scenes, null);
        if (cycle === 0) result.project = data;
        else assert.deepEqual(data, result.project);
        assert.deepEqual(await readFile(source), bytes);
        assert.deepEqual(await readFile(legacy), legacyBytes);
        await save();
      }
      await browser.saveScreenshot(
        join(evidence, `import-${kind}-10000-review.png`),
      );
      result.timings.projectOpenMs = await browser.execute(async (source) => {
        const b = window.__legacyBench,
          at = performance.now();
        await b.click('Open created project');
        await b.wait(async () => {
          const d = await b.state();
          return (
            d.path === source &&
            d.cues.length === 10000 &&
            b.ready() &&
            document
              .querySelector('footer [role="status"]')
              ?.textContent.includes('Project restored')
          );
        });
        await b.frame();
        return performance.now() - at;
      }, source);
      const baseline = await status();
      assert.equal(baseline.format.name, format);
      assert.equal(baseline.cues.length, 10000);
      const newline =
        kind === 'styled' ? '\r\n' : kind === 'timed' ? '\r' : '\n';
      const rawFirst = baseline.cues[0].text.replaceAll('\n', newline);
      assert(original.includes(rawFirst));
      const edited = `Edited ${baseline.cues[0].text}`;
      const expectedSaved = original.replace(
        rawFirst,
        edited.replaceAll('\n', newline),
      );
      Object.assign(
        result.timings,
        await browser.execute(
          async (text, copy) => {
            const b = window.__legacyBench,
              before = await b.state();
            document.querySelector('.cue-row').click();
            await b.wait(
              () =>
                document.querySelector('[aria-label="Subtitle text"]').value ===
                before.cues[0].text,
            );
            b.input(text);
            await b.frame();
            let at = performance.now();
            await b.click('Apply edit');
            const accepted = await b.wait(async () => {
              const d = await b.state();
              return d.revision === before.revision + 1 && b.ready() && d;
            });
            await b.frame();
            const editMs = performance.now() - at;
            if (accepted.cues[0].text !== text)
              throw new Error('Imported edit failed');
            const undoMs = await b.history('Undo edit', accepted.revision);
            if (
              JSON.stringify((await b.state()).cues) !==
              JSON.stringify(before.cues)
            )
              throw new Error('Imported Undo lost cues');
            const redoMs = await b.history('Redo edit', accepted.revision + 1);
            if (
              JSON.stringify((await b.state()).cues) !==
              JSON.stringify(accepted.cues)
            )
              throw new Error('Imported Redo lost cues');
            at = performance.now();
            await b.menu('saveAs');
            await b.wait(async () => {
              const d = await b.state();
              return d.path === copy && !d.dirty && b.ready();
            });
            await b.frame();
            return { editMs, undoMs, redoMs, saveMs: performance.now() - at };
          },
          edited,
          copy,
        ),
      );
      assert.equal(await readFile(copy, 'utf8'), expectedSaved);
      result.timings.projectReopenMs = await browser.execute(async (source) => {
        const b = window.__legacyBench,
          at = performance.now();
        await b.menu('openProject');
        await b.wait(
          async () =>
            (await b.state()).path === source &&
            b.ready() &&
            document
              .querySelector('footer [role="status"]')
              ?.textContent.includes('Project restored'),
        );
        await b.frame();
        return performance.now() - at;
      }, source);
      assert.deepEqual((await status()).cues, baseline.cues);
      const recoveryText = `Recovered ${baseline.cues[0].text}`;
      const expectedRecovery = original.replace(
        rawFirst,
        recoveryText.replaceAll('\n', newline),
      );
      result.timings.recoveryEditMs = await browser.execute(async (text) => {
        const b = window.__legacyBench,
          before = await b.state();
        document.querySelector('.cue-row').click();
        await b.wait(
          () =>
            document.querySelector('[aria-label="Subtitle text"]').value ===
            before.cues[0].text,
        );
        b.input(text);
        await b.frame();
        const at = performance.now();
        await b.click('Apply edit');
        await b.wait(
          async () =>
            (await b.state()).revision === before.revision + 1 && b.ready(),
        );
        await b.frame();
        return performance.now() - at;
      }, recoveryText);
      const accepted = await status();
      assert.equal(accepted.cues[0].text, recoveryText);
      await until('exact imported 10k recovery checkpoint', async () => {
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
              snapshot.format === format &&
              snapshot.generation === accepted.generation &&
              snapshot.revision === accepted.revision &&
              snapshot.accepted_srt === expectedRecovery
            )
              return true;
          } catch (e) {
            if (e.code !== 'ENOENT') throw e;
          }
        }
        return false;
      });
      browser = await restart([{ kind: 'saveSubtitles', path: recovered }]);
      await install(browser);
      result.timings.recoveryOpenMs = await browser.execute(async (source) => {
        const b = window.__legacyBench;
        const row = await b.wait(() =>
          [...document.querySelectorAll('.recovery-row')].find(
            (r) => r.querySelector('span')?.title === source,
          ),
        );
        const at = performance.now();
        [...row.querySelectorAll('button')]
          .find((b) => b.textContent.trim() === 'Recover')
          .click();
        await b.wait(async () => {
          const d = await b.state();
          return d.requiresSaveAs && d.cues.length === 10000 && b.ready();
        });
        await b.frame();
        return performance.now() - at;
      }, source);
      assert.deepEqual((await status()).cues, accepted.cues);
      result.timings.recoverySaveMs = await browser.execute(async (path) => {
        const b = window.__legacyBench,
          at = performance.now();
        await b.menu('saveAs');
        await b.wait(async () => {
          const d = await b.state();
          return d.path === path && !d.requiresSaveAs && !d.dirty && b.ready();
        });
        await b.frame();
        return performance.now() - at;
      }, recovered);
      assert.equal(await readFile(recovered, 'utf8'), expectedRecovery);
      const request = { path: legacy, mediaPath: null, subtitlePath: null };
      const rejected = join(root, `${kind}-rejected.vsfproject`);
      const guards = await browser.execute(
        async (request, rejected) => {
          const b = window.__legacyBench;
          const id = await b.invoke('project_import_start', { request });
          const at = performance.now();
          await b.invoke('project_import_cancel', { id });
          await b.frame();
          const cancellationMs = performance.now() - at;
          await b.wait(async () => {
            const v = await b.invoke('project_import_view', { id });
            return !v.running && v.phase === 'cancelled';
          });
          const cancellationDrainedMs = performance.now() - at;
          let refused = false;
          try {
            await b.invoke('project_import_publish', { id, path: rejected });
          } catch {
            refused = true;
          }
          if (!refused) throw new Error('Cancelled import published');
          const next = await b.invoke('project_import_start', { request });
          await b.wait(async () => {
            const v = await b.invoke('project_import_view', { id: next });
            return !v.running && v.phase === 'reviewing';
          });
          return { cancellationMs, cancellationDrainedMs, staleId: next };
        },
        request,
        rejected,
      );
      const changed = Buffer.concat([bytes, Buffer.from('\n')]);
      await writeFile(source, changed);
      result.timings.staleRejectionMs = await browser.execute(
        async (id, path) => {
          const b = window.__legacyBench,
            at = performance.now();
          await b.invoke('project_import_publish', { id, path });
          const view = await b.wait(async () => {
            const v = await b.invoke('project_import_view', { id });
            return !v.running && v;
          });
          if (view.phase !== 'stale' || view.outputPath !== null)
            throw new Error('Changed original was not rejected');
          await b.frame();
          return performance.now() - at;
        },
        guards.staleId,
        rejected,
      );
      assert.deepEqual(await readFile(source), changed);
      await assert.rejects(readFile(rejected), { code: 'ENOENT' });
      await writeFile(source, bytes);
      result.timings.cancellationMs = guards.cancellationMs;
      result.timings.cancellationDrainedMs = guards.cancellationDrainedMs;
      assert.deepEqual(await readFile(source), bytes);
      assert.deepEqual(await readFile(legacy), legacyBytes);
      assert.equal(hash(await readFile(media)), mediaHash);
      assert.equal(await readFile(active, 'utf8'), activeBytes);
      result.outputSha256 = hash(expectedRecovery);
      result.visibleRows = await browser.execute(
        () => document.querySelectorAll('.cue-row').length,
      );
      assert(result.visibleRows > 0 && result.visibleRows <= 200);
      result.violations = [];
      for (const times of [result.timings, ...result.cycles]) {
        for (const [name, ms] of Object.entries(times)) {
          const budget =
            name === 'preparationMs'
              ? report.preparationDeadlineMs
              : report.budgetMs;
          if (ms !== null && ms >= budget)
            result.violations.push(`${name}: ${ms}ms >= ${budget}ms`);
        }
      }
      result.status = result.violations.length ? 'failed' : 'passed';
      await save();
      await record({
        step: `10k legacy ${kind} import/publish/project/history/recovery/cancel/stale`,
        ...result,
      });
    }
    assert.deepEqual(
      report.workloads.flatMap((r) =>
        r.violations.map((v) => `${r.kind} ${v}`),
      ),
      [],
      'Desktop import performance gates failed',
    );
    report.status = 'passed';
  } catch (error) {
    report.status = 'failed';
    report.error = `${String(error)}\n${error.stack ?? ''}`;
    throw error;
  } finally {
    await save();
  }
}
