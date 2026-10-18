// Complete real desktop plugin workloads. File choices and DOM events are supplied;
// this is not physical input/GPU/audio acceptance. Run without other CPU benchmarks.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { promisify } from 'node:util';
const run = promisify(execFile);
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const fixtures = [
  [
    'dense',
    'srt',
    null,
    'c8118a0cc80dc2e2955c68d83401a7cb1e80f0858fc6d5ef45827a3c1bfec6e2',
  ],
  [
    'ass',
    'ass',
    'ass-ssa/10000.ass',
    '00e4deb1210c47e2943c30744da0e5d0512a57ba159b659d9dd16ef5d47c1c0c',
  ],
  [
    'ssa',
    'ssa',
    'ass-ssa/10000.ssa',
    '0d2415e1e2d61776e7c9051c3c3c1c10b1c02770e225652b7b0132c57fb79b14',
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
];

// Independent byte oracle for these hash-pinned fixtures, not a product serializer.
function changedBytes(original, format) {
  const shift = (stamp, tick, separator) => {
    const [h, m, s, fraction] = stamp.split(/[:,.]/).map(Number);
    const ms = ((h * 60 + m) * 60 + s) * 1000 + fraction * tick + 10;
    return `${String(Math.floor(ms / 3600000)).padStart(format === 'srt' ? 2 : 1, '0')}:${String(Math.floor(ms / 60000) % 60).padStart(2, '0')}:${String(Math.floor(ms / 1000) % 60).padStart(2, '0')}${separator}${String(Math.floor((ms % 1000) / tick)).padStart(tick === 1 ? 3 : 2, '0')}`;
  };
  if (format === 'ass' || format === 'ssa')
    return original
      .split('\n')
      .map((line) => {
        if (!line.startsWith('Dialogue: ')) return line;
        const fields = line.split(',');
        fields[1] = shift(fields[1], 10, '.');
        fields[2] = shift(fields[2], 10, '.');
        return fields.join(',') + '!';
      })
      .join('\n');
  // Timed WebVTT deliberately mixes all three newline forms. Retain each
  // separator independently, including empty lines between opaque blocks.
  const lines = [...original.matchAll(/([^\r\n]*)(\r\n|\r|\n|$)/g)].map(
    (match) => ({ body: match[1], ending: match[2] }),
  );
  let payload = false,
    last = null,
    changed = 0;
  for (const line of lines) {
    if (line.body.includes(' --> ')) {
      payload = true;
      if (format === 'srt')
        line.body = line.body.replace(/\d{2}:\d{2}:\d{2},\d{3}/g, (stamp) =>
          shift(stamp, 1, ','),
        );
    } else if (payload && line.body === '') {
      assert(last, 'Fixture cue has no payload');
      last.body += '!';
      ++changed;
      last = null;
      payload = false;
    } else if (payload) last = line;
  }
  assert.equal(changed, 10000, 'Byte oracle must change every fixture cue');
  return lines.map(({ body, ending }) => body + ending).join('');
}

async function sampleMemory(pid) {
  const report = { pid, samples: [], helperSamples: 0 };
  const sample = async () => {
    const { stdout } = await run('ps', ['-axo', 'pid=,ppid=,rss=,command='], {
      timeout: 2000,
    });
    let appBytes = 0,
      helperBytes = 0,
      helpers = 0;
    for (const line of stdout.split('\n')) {
      const m = /^\s*(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/.exec(line);
      if (!m) continue;
      if (Number(m[1]) === pid) appBytes = Number(m[3]) * 1024;
      if (Number(m[2]) === pid && m[4].includes('--vsf-plugin-worker')) {
        helperBytes += Number(m[3]) * 1024;
        ++helpers;
      }
    }
    assert(appBytes > 0, 'Owned app missing from RSS sample');
    assert(helpers <= 1, 'More than one admitted helper');
    report.helperSamples += Number(helpers > 0);
    return {
      at: Date.now(),
      appBytes,
      helperBytes,
      combinedBytes: appBytes + helperBytes,
    };
  };
  report.baseline = await sample();
  let active = true;
  const task = (async () => {
    while (active) {
      try {
        report.samples.push(await sample());
      } catch (e) {
        report.error = String(e);
        break;
      }
      await delay(50);
    }
  })();
  return {
    report,
    stop: async () => {
      active = false;
      await task;
      report.peakCombinedBytes = Math.max(
        report.baseline.combinedBytes,
        ...report.samples.map((s) => s.combinedBytes),
      );
      report.additionalCombinedBytes =
        report.peakCombinedBytes - report.baseline.combinedBytes;
      assert.equal(report.error, undefined, 'RSS sampling failed');
    },
  };
}

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
      dialog: () => document.querySelector('#plugin-title')?.closest('dialog'),
      button: (label) =>
        [...document.querySelectorAll('button')].find(
          (b) => b.textContent.trim() === label && !b.disabled,
        ),
      wait: async (probe) => {
        const deadline = performance.now() + 9500;
        while (performance.now() < deadline) {
          const result = await probe();
          if (result) return result;
          await new Promise((r) => setTimeout(r, 10));
        }
        throw new Error(
          `Plugin 10k operation did not settle: ${document.querySelector('dialog')?.textContent ?? document.querySelector('footer')?.textContent}`,
        );
      },
      input: (label, text) => {
        const input = document.querySelector(`[aria-label="${label}"]`);
        if (!input || input.disabled) throw new Error(`Unavailable ${label}`);
        input.value = text;
        input.dispatchEvent(new Event('input', { bubbles: true }));
      },
      assert: (value, message) => {
        if (!value) throw new Error(message);
      },
    };
    b.click = async (label) => (await b.wait(() => b.button(label))).click();
    b.open = async (path) => {
      await b.wait(b.ready);
      await b.menu('plugins');
      await b.wait(b.dialog);
      await b.click('Choose plugin…');
      await b.wait(
        () =>
          document.querySelector('[aria-label="Plugin path"]')?.textContent ===
            path && b.button('Choose plugin…'),
      );
      b.dialog().querySelector('fieldset details').open = true;
      b.input('Plugin CPS target', '20');
      await b.frame();
    };
    b.review = async (fix) => {
      const at = performance.now();
      await b.click(fix ? 'Preview plugin fixes' : 'Run checker');
      await b.frame();
      const startMs = performance.now() - at;
      await b.wait(() => {
        const alert = b.dialog()?.querySelector('[role="alert"]');
        if (alert) throw new Error(alert.textContent);
        return (
          b.dialog()?.textContent.includes('Plugin: reviewing') &&
          (fix
            ? b.button('Accept plugin changes')
            : b
                .dialog()
                .textContent.includes('Checking does not change subtitles.'))
        );
      });
      await b.frame();
      return { startMs, preparationMs: performance.now() - at };
    };
    b.accept = async () => {
      const before = await b.state(),
        at = performance.now();
      await b.click('Accept plugin changes');
      await b.wait(
        async () =>
          (await b.state()).revision === before.revision + 1 && b.ready(),
      );
      await b.frame();
      return performance.now() - at;
    };
    b.history = async (label) => {
      const before = await b.state(),
        at = performance.now();
      await b.click(label);
      await b.wait(
        async () =>
          (await b.state()).revision === before.revision + 1 && b.ready(),
      );
      await b.frame();
      return performance.now() - at;
    };
    b.save = async (path, as) => {
      const at = performance.now();
      await b.menu(as ? 'saveAs' : 'save');
      await b.wait(async () => {
        const d = await b.state();
        return d.path === path && !d.dirty && b.ready();
      });
      await b.frame();
      return performance.now() - at;
    };
    b.view = (id) =>
      invoke('plugin_view', {
        id,
        findingsOffset: 0,
        changesOffset: 0,
        limit: 50,
      });
    b.finish = (id) =>
      b.wait(async () => {
        const v = await b.view(id);
        return !['preparing', 'accepting', 'cancelling'].includes(v.phase) && v;
      });
    b.page = async (kind, offset, shift) => {
      const section = () =>
        b.dialog().querySelector(`[aria-label="Plugin ${kind}"]`);
      const at = performance.now();
      if (offset > 0) await b.click(`Next ${kind}`);
      const rows = await b.wait(() => {
        const rows = [
          ...section().querySelectorAll(kind === 'findings' ? 'li' : 'article'),
        ];
        const text = section()
          .querySelector('.actions span')
          ?.textContent.replace(/\s/g, '');
        return (
          rows.length === 50 &&
          text === `${offset + 1}–${offset + 50}of10000` &&
          rows
        );
      });
      for (let i = 0; i < rows.length; i++) {
        const cue = b.baseline.cues[offset + i];
        if (kind === 'findings')
          b.assert(
            rows[i].textContent.trim() === `Cue ID ${cue.id}: append`,
            'Finding lost/reordered cue',
          );
        else {
          b.assert(
            rows[i].querySelector('h4').textContent.trim() ===
              `Cue ${offset + i + 1} · ID ${cue.id}`,
            'Change lost/reordered cue',
          );
          const text = [...rows[i].querySelectorAll('pre')].map(
            (p) => p.textContent,
          );
          b.assert(
            text[0] === cue.text && text[1] === cue.text + '!',
            'Raw review text differs',
          );
          const time = (ms) =>
            `${String(Math.floor(ms / 3600000)).padStart(2, '0')}:${String(Math.floor(ms / 60000) % 60).padStart(2, '0')}:${String(Math.floor(ms / 1000) % 60).padStart(2, '0')},${String(ms % 1000).padStart(3, '0')}`;
          const times = [...rows[i].querySelectorAll('.comparison p')].map(
            (p) => p.textContent.trim(),
          );
          b.assert(
            times[0] === `${time(cue.startMs)} → ${time(cue.endMs)}` &&
              times[1] ===
                `${time(cue.startMs + shift)} → ${time(cue.endMs + shift)}`,
            'Review timing differs',
          );
        }
      }
      await b.frame();
      return {
        kind,
        first: offset + 1,
        last: offset + 50,
        ms: performance.now() - at,
      };
    };
    window.__pluginBench = b;
  });
}

export async function pluginPerformance({
  restart,
  status,
  until,
  root,
  evidence,
  binary,
  record,
  getPid,
}) {
  const selected =
    process.env.VSF_PLUGIN_PERFORMANCE_WORKLOADS?.split(',') ??
    fixtures.map(([kind]) => kind);
  assert(
    selected.length &&
      selected.every((v) => fixtures.some(([kind]) => kind === v)),
    'Unknown or empty workload selection',
  );
  const runner = join(
    dirname(binary),
    `vsf${process.platform === 'win32' ? '.exe' : ''}`,
  );
  const report = {
    status: 'running',
    budgetMs: 1000,
    preparationDeadlineMs: 10000,
    selectedWorkloads: selected,
    binary,
    binarySha256: hash(await readFile(binary)),
    runnerSha256: hash(await readFile(runner)),
    scope:
      'Actual WebView/Rust/installed helper, both APIs, exact10k fixtures. Full findings/change UI paging; five combined edit/history/save cycles, reopen, crash recovery, cancellation and stale rejection. No media. Supplied chooser/DOM input. Sampled RSS covers owned app and direct plugin helper, excludes WebKit/native subprocesses; descriptive only. Existing pure/API/CLI500ms/192MiB gates unchanged.',
    workloads: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'plugin-performance.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const checkTimes = (values) => {
    for (const [name, ms] of Object.entries(values))
      assert(
        ms < (name.endsWith('PreparationMs') ? 10000 : 1000),
        `${name} exceeded budget: ${ms}ms`,
      );
  };
  let browser, memory;
  try {
    for (const [kind, format, fixture, digest] of fixtures.filter(([kind]) =>
      selected.includes(kind),
    ))
      for (const api of ['vsf-1', 'vss-1']) {
        const bytes = fixture
          ? await readFile(resolve('tests/fixtures/generated', fixture))
          : Buffer.from(
              Array.from(
                { length: 10000 },
                (_, i) =>
                  `${i + 1}\n00:00:01,000 --> 00:00:05,000\n{\\an8\\k20}<i>Été 世界 😀 ${i + 1}</i>\nSecond line\n\n`,
              ).join(''),
            );
        assert.equal(hash(bytes), digest);
        const original = bytes.toString('utf8'),
          label = `${kind}-${api}`,
          source = join(root, `source-${label}.${format}`),
          output = join(root, `output-${label}.${format}`),
          recovered = join(root, `recovered-${label}.${format}`),
          directory = join(root, `plugin-${label}`),
          manifest = join(directory, 'manifest.json'),
          shift = format === 'vtt' ? 0 : 10;
        const expected = changedBytes(original, format),
          expectedRecovery = changedBytes(expected, format);
        await writeFile(source, bytes, { flag: 'wx' });
        await mkdir(directory);
        await writeFile(
          manifest,
          JSON.stringify({
            version: 1,
            id: 'test.scale',
            name: 'Desktop scale',
            api,
            entry: 'main.js',
            requires: ['rules', 'fixes'],
          }),
        );
        await writeFile(
          join(directory, 'main.js'),
          api === 'vsf-1'
            ? `VSFPlugin={check(){return 'append'},fix(c,h){h.update(c.id,{text:c.text+'!',startMs:c.startMs+${shift},endMs:c.endMs+${shift}})}}`
            : `VSSPlugin={Name:'Desktop scale',HasError(){return 'append'},FixError(c){c.Text+='!';c.Start+=${shift};c.Stop+=${shift};}}`,
        );
        browser = await restart([
          { kind: 'subtitles', path: source },
          { kind: 'plugin', path: manifest },
          { kind: 'saveSubtitles', path: null },
          {
            kind: 'saveSubtitles',
            path: join(root, 'missing-parent', `failed.${format}`),
          },
          { kind: 'saveSubtitles', path: output },
          ...Array.from({ length: 4 }, () => ({
            kind: 'plugin',
            path: manifest,
          })),
          { kind: 'subtitles', path: output },
          { kind: 'plugin', path: manifest },
        ]);
        await install(browser);
        const result = {
          kind,
          format,
          api,
          sourceSha256: digest,
          inputBytes: bytes.length,
          cycles: [],
          pages: [],
          timings: {},
          memory: [],
        };
        report.workloads.push(result);
        memory = await sampleMemory(getPid());
        result.memory.push(memory.report);
        await save();
        console.log(
          `Plugin10k ${label}: open and five check/fix/history/save cycles`,
        );
        result.timings.openMs = await browser.execute(async (source) => {
          const b = window.__pluginBench,
            at = performance.now();
          await b.menu('openSubtitles');
          const d = await b.wait(async () => {
            const d = await b.state();
            return (
              d.path === source && d.cues.length === 10000 && b.ready() && d
            );
          });
          b.baseline = d;
          await b.frame();
          return performance.now() - at;
        }, source);
        const baseline = await status();
        const expectedCues = baseline.cues.map((c) => ({
          ...c,
          text: c.text + '!',
          startMs: c.startMs + shift,
          endMs: c.endMs + shift,
        }));
        checkTimes(result.timings);
        for (let cycle = 0; cycle < 5; cycle++) {
          const beforeCheck = await status();
          const times = await browser.execute(async (manifest) => {
            const b = window.__pluginBench;
            await b.open(manifest);
            const check = await b.review(false);
            return {
              checkStartMs: check.startMs,
              checkPreparationMs: check.preparationMs,
            };
          }, manifest);
          result.cycles.push(times);
          assert.deepEqual(await status(), beforeCheck);
          if (cycle === 0) {
            for (let batch = 0; batch < 40; batch++) {
              const pages = await browser.execute(async (offset) => {
                const pages = [];
                for (let i = offset; i < offset + 250; i += 50)
                  pages.push(await window.__pluginBench.page('findings', i, 0));
                return pages;
              }, batch * 250);
              result.pages.push(...pages);
              await save();
              for (const page of pages) checkTimes({ pageMs: page.ms });
            }
          }
          const fix = await browser.execute(() =>
            window.__pluginBench.review(true),
          );
          Object.assign(times, {
            fixStartMs: fix.startMs,
            fixPreparationMs: fix.preparationMs,
          });
          if (cycle === 0) {
            for (let batch = 0; batch < 40; batch++) {
              const pages = await browser.execute(
                async (offset, shift) => {
                  const pages = [];
                  for (let i = offset; i < offset + 250; i += 50)
                    pages.push(
                      await window.__pluginBench.page('changes', i, shift),
                    );
                  return pages;
                },
                batch * 250,
                shift,
              );
              result.pages.push(...pages);
              await save();
              for (const page of pages) checkTimes({ pageMs: page.ms });
            }
          }
          times.acceptMs = await browser.execute(() =>
            window.__pluginBench.accept(),
          );
          const accepted = await status();
          assert.deepEqual(accepted.cues, expectedCues);
          assert.deepEqual(accepted.format, baseline.format);
          times.undoMs = await browser.execute(() =>
            window.__pluginBench.history('Undo edit'),
          );
          assert.deepEqual((await status()).cues, baseline.cues);
          times.redoMs = await browser.execute(() =>
            window.__pluginBench.history('Redo edit'),
          );
          assert.deepEqual((await status()).cues, expectedCues);
          if (cycle === 0) {
            const before = await status();
            await browser.execute(async () => {
              const b = window.__pluginBench;
              await b.menu('saveAs');
              await b.frame();
              await b.wait(b.ready);
              await b.menu('saveAs');
              await b.wait(
                () =>
                  b.ready() &&
                  /directory|No such|cannot/i.test(
                    document.querySelector('footer [role="status"]')
                      ?.textContent ?? '',
                  ),
              );
            });
            assert.deepEqual(await status(), before);
            result.failedSavePreserved = true;
          }
          times.saveMs = await browser.execute(
            (output, as) => window.__pluginBench.save(output, as),
            output,
            cycle === 0,
          );
          assert.equal(await readFile(output, 'utf8'), expected);
          assert.deepEqual(await readFile(source), bytes);
          times.restoreMs = await browser.execute(() =>
            window.__pluginBench.history('Undo edit'),
          );
          assert.deepEqual((await status()).cues, baseline.cues);
          await save();
          checkTimes(times);
        }
        await browser.execute(() => window.__pluginBench.history('Redo edit'));
        const saved = await status();
        result.timings.reopenMs = await browser.execute(async (output) => {
          const b = window.__pluginBench,
            generation = (await b.state()).generation,
            at = performance.now();
          await b.menu('openSubtitles');
          await b.wait(async () => {
            const d = await b.state();
            return (
              d.path === output &&
              d.generation > generation &&
              d.cues.length === 10000 &&
              b.ready()
            );
          });
          await b.frame();
          return performance.now() - at;
        }, output);
        assert.deepEqual((await status()).cues, saved.cues);
        assert.deepEqual((await status()).format, saved.format);
        Object.assign(
          result.timings,
          await browser.execute(async (manifest) => {
            const b = window.__pluginBench,
              before = await b.state();
            const input = {
              mode: 'fix',
              scope: [],
              selection: [],
              cursorMs: null,
              settings: {},
              cpsTarget: 20,
              rsTargetIndex: null,
              sceneFilterOffsetMs: 0,
            };
            const start = () =>
              b.invoke('plugin_start', {
                session: 0,
                generation: before.generation,
                revision: before.revision,
                path: manifest,
                input,
              });
            const id = await start(),
              at = performance.now();
            await b.invoke('plugin_cancel', { id });
            const phase = (await b.finish(id)).phase;
            await b.frame();
            const cancelMs = performance.now() - at;
            b.assert(phase === 'cancelled', `Unexpected cancel phase ${phase}`);
            let refused = false;
            try {
              await b.invoke('plugin_accept', { id, input });
            } catch {
              refused = true;
            }
            b.assert(
              refused &&
                JSON.stringify(await b.state()) === JSON.stringify(before),
              'Cancelled plugin changed state',
            );
            const stale = await start();
            const review = await b.finish(stale);
            b.assert(
              review.phase === 'reviewing',
              `Stale control preparation: ${review.error}`,
            );
            b.input('Subtitle text', `Stale control ${before.cues[0].text}`);
            await b.click('Apply edit');
            await b.wait(
              async () =>
                (await b.state()).revision === before.revision + 1 && b.ready(),
            );
            const changed = await b.state(),
              staleAt = performance.now();
            refused = false;
            try {
              await b.invoke('plugin_accept', { id: stale, input });
            } catch {
              refused = true;
            }
            const staleRejectionMs = performance.now() - staleAt;
            b.assert(
              refused &&
                JSON.stringify(await b.state()) === JSON.stringify(changed),
              'Stale acceptance changed state',
            );
            await b.history('Undo edit');
            b.assert(
              JSON.stringify((await b.state()).cues) ===
                JSON.stringify(before.cues),
              'Stale control Undo lost state',
            );
            return { cancelMs, staleRejectionMs };
          }, manifest),
        );
        await browser.execute(async (manifest) => {
          const b = window.__pluginBench;
          await b.open(manifest);
          await b.review(true);
          await b.accept();
        }, manifest);
        const checkpoint = await status();
        assert.deepEqual(
          checkpoint.cues,
          expectedCues.map((c) => ({
            ...c,
            text: c.text + '!',
            startMs: c.startMs + shift,
            endMs: c.endMs + shift,
          })),
        );
        await until('complete plugin10k recovery checkpoint', async () => {
          for (const entry of await readdir(join(root, 'recovery-v1'), {
            withFileTypes: true,
          })) {
            if (!entry.isDirectory()) continue;
            try {
              const bytes = await readFile(
                join(root, 'recovery-v1', entry.name, 'snapshot.json'),
              );
              const d = JSON.parse(bytes.toString('utf8'));
              if (
                d.source_path === output &&
                d.generation === checkpoint.generation &&
                d.revision === checkpoint.revision &&
                d.accepted_srt === expectedRecovery
              ) {
                result.checkpointBytes = bytes.length;
                return true;
              }
            } catch (e) {
              if (e.code !== 'ENOENT') throw e;
            }
          }
          return false;
        });
        await memory.stop();
        memory = null;
        await save();
        browser = await restart([{ kind: 'saveSubtitles', path: recovered }]);
        await install(browser);
        memory = await sampleMemory(getPid());
        result.memory.push(memory.report);
        result.timings.recoveryOpenMs = await browser.execute(
          async (output) => {
            const b = window.__pluginBench;
            const row = await b.wait(() =>
              [...document.querySelectorAll('.recovery-row')].find(
                (r) => r.querySelector('span')?.title === output,
              ),
            );
            const at = performance.now();
            [...row.querySelectorAll('button')]
              .find((e) => e.textContent.trim() === 'Recover')
              .click();
            await b.wait(async () => {
              const d = await b.state();
              return d.requiresSaveAs && d.cues.length === 10000 && b.ready();
            });
            await b.frame();
            return performance.now() - at;
          },
          output,
        );
        const restored = await status();
        assert.equal(restored.canUndo, false);
        assert.deepEqual(restored.cues, checkpoint.cues);
        assert.deepEqual(restored.format, checkpoint.format);
        result.timings.recoverySaveMs = await browser.execute(
          (output) => window.__pluginBench.save(output, true),
          recovered,
        );
        assert.equal(await readFile(recovered, 'utf8'), expectedRecovery);
        assert.equal(await readFile(output, 'utf8'), expected);
        assert.deepEqual(await readFile(source), bytes);
        await memory.stop();
        memory = null;
        Object.assign(result, {
          outputSha256: hash(expected),
          recoverySha256: hash(expectedRecovery),
          status: 'passed',
        });
        await save();
        checkTimes(result.timings);
        await record({
          step: `plugin10k ${label}: full pages, five check/fix/history/save cycles, reopen, cancellation/stale, recovery`,
          timings: result.timings,
        });
      }
    report.status = 'passed';
    await save();
  } catch (e) {
    if (memory) await memory.stop().catch(() => {});
    report.status = 'failed';
    report.error = String(e.stack ?? e);
    await save();
    throw e;
  }
}
