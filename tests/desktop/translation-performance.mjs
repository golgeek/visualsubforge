// Actual WebView/Rust/files with simultaneous target/reference/history/recovery.
// Supplied chooser results and DOM events do not measure physical input latency.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, readdir, unlink, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
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
  [
    'dense',
    'srt',
    null,
    'ec3e642d86941c902dd4cb995ebc02fcc55ec2f9bcb3f8b53ac62b6d8dc76f05',
  ],
];

async function sampleMemory(pid, phase) {
  const rss = async () => {
    const { stdout } = await run('ps', ['-o', 'rss=', '-p', String(pid)], {
      timeout: 2000,
    });
    const value = Number(stdout.trim()) * 1024;
    assert(Number.isFinite(value) && value > 0, 'Invalid application RSS');
    return value;
  };
  const report = { phase, pid, baselineRssBytes: await rss(), samples: [] };
  let active = true;
  const task = (async () => {
    while (active) {
      try {
        report.samples.push({ at: Date.now(), bytes: await rss() });
      } catch (error) {
        report.error = String(error);
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
      report.peakRssBytes = Math.max(
        report.baselineRssBytes,
        ...report.samples.map((s) => s.bytes),
      );
      report.additionalRssBytes = report.peakRssBytes - report.baselineRssBytes;
      assert.equal(report.error, undefined, 'RSS sampler failed');
    },
  };
}

async function install(browser) {
  await browser.execute((profileDraws) => {
    if (profileDraws) {
      const draws = [],
        active = new WeakMap();
      const width = Object.getOwnPropertyDescriptor(
        HTMLCanvasElement.prototype,
        'width',
      );
      Object.defineProperty(HTMLCanvasElement.prototype, 'width', {
        ...width,
        set(value) {
          const draw = {
            start: performance.now(),
            width: value,
            labels: 0,
            labelMs: 0,
            rectangles: 0,
            rectangleMs: 0,
          };
          active.set(this, draw);
          width.set.call(this, value);
          queueMicrotask(() => {
            draw.microtaskMs = performance.now() - draw.start;
            if (draws.length < 2000) draws.push(draw);
          });
        },
      });
      for (const name of ['fillText', 'fillRect', 'strokeRect']) {
        const original = CanvasRenderingContext2D.prototype[name];
        CanvasRenderingContext2D.prototype[name] = function (...args) {
          const at = performance.now(),
            result = original.apply(this, args),
            draw = active.get(this.canvas);
          if (draw) {
            if (name === 'fillText') {
              ++draw.labels;
              draw.labelMs += performance.now() - at;
            } else {
              ++draw.rectangles;
              draw.rectangleMs += performance.now() - at;
            }
          }
          return result;
        };
      }
      window.__translationDraws = draws;
    }
    const invoke = window.__TAURI__.core.invoke;
    const b = {
      invoke,
      state: () =>
        invoke('document_action', {
          action: { kind: 'status' },
          generation: 0,
          revision: 0,
        }),
      translation: (action) => invoke('translation_action', { action }),
      key: (d) => ({
        generation: d.generation,
        revision: d.revision,
        epoch: d.translation?.epoch ?? 0,
      }),
      frame: () => new Promise((r) => requestAnimationFrame(r)),
      menu: (action) => window.__TAURI__.event.emit('workspace-action', action),
      ready: () =>
        document.querySelector('main')?.getAttribute('aria-busy') === 'false' &&
        !document.querySelector('dialog[open]'),
      button: (label) =>
        [...document.querySelectorAll('button')].find(
          (e) => e.textContent.trim() === label && !e.disabled,
        ),
      input: (label, text) => {
        const e = document.querySelector(`[aria-label="${label}"]`);
        if (!e) throw new Error(`Missing ${label}`);
        e.value = text;
        e.dispatchEvent(new Event('input', { bubbles: true }));
      },
      wait: async (probe) => {
        const until = performance.now() + 9500;
        while (performance.now() < until) {
          const value = await probe();
          if (value) return value;
          await new Promise((r) => setTimeout(r, 10));
        }
        throw new Error('Translation desktop measurement did not settle');
      },
      panel: () =>
        document.querySelector('[aria-label="Translation reference"]'),
      rows: () => [
        ...document.querySelectorAll('[aria-label="Reference cues"] button'),
      ],
    };
    b.click = async (label) => (await b.wait(() => b.button(label))).click();
    b.page = async (first, count) =>
      b.wait(() => {
        const panel = b.panel(),
          rows = b.rows();
        const error = panel?.querySelector('[role="alert"]');
        if (error) throw new Error(error.textContent);
        return (
          rows.length === count &&
          rows[0]
            ?.querySelector('strong')
            ?.textContent.trim()
            .startsWith(`#${first} ·`) &&
          b.ready()
        );
      });
    b.history = async (label, revision) => {
      const at = performance.now();
      await b.click(label);
      await b.wait(
        async () => (await b.state()).revision === revision + 1 && b.ready(),
      );
      await b.frame();
      return performance.now() - at;
    };
    b.offset = async (offset) => {
      b.input('Reference offset in milliseconds', String(offset));
      const at = performance.now();
      await b.click('Apply offset');
      await b.wait(
        async () =>
          (await b.state()).translation?.reference?.offsetMs === offset &&
          b.ready(),
      );
      await b.page(1, 100);
      await b.frame();
      return performance.now() - at;
    };
    b.finish = async (id) =>
      b.wait(async () => {
        const reply = await b.translation({ kind: 'view', id });
        return (
          reply.job &&
          !['working', 'accepting', 'cancelling'].includes(reply.job.phase) &&
          reply
        );
      });
    window.__translationBench = b;
  }, process.env.VSF_TRANSLATION_PROFILE_DRAW === '1');
}

export async function translationPerformance({
  restart,
  status,
  until,
  root,
  evidence,
  binary,
  record,
  getPid,
}) {
  const selectedWorkloads =
    process.env.VSF_TRANSLATION_PERFORMANCE_WORKLOADS?.split(',') ??
    fixtures.map(([kind]) => kind);
  assert(
    selectedWorkloads.length > 0 &&
      selectedWorkloads.every((kind) =>
        fixtures.some(([name]) => name === kind),
      ),
    'Unknown or empty translation workload selection',
  );
  const report = {
    schema: 1,
    status: 'running',
    budgetMs: 1000,
    preparationDeadlineMs: 10000,
    selectedWorkloads,
    diagnosticDrawProfile: process.env.VSF_TRANSLATION_PROFILE_DRAW === '1',
    binary,
    binarySha256: hash(await readFile(binary)),
    scope:
      'Actual WebView/Rust/files; selectedWorkloads lists the exact 10k fixtures exercised. Full reference paging, five offset/edit/history/save/project cycles, full project reopen and captured-reference recovery, cancellation/stale/source guards. Supplied file choices and DOM input. No media loaded. Sampled RSS covers the app process with accepted target/reference/history/checkpoint state; excludes WebKit subprocesses. The separate release pure-engine 500ms p95/96MiB gates are unchanged.',
    workloads: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'translation-performance.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  let memory, browser;
  try {
    for (const [kind, format, fixture, digest] of fixtures.filter(([kind]) =>
      selectedWorkloads.includes(kind),
    )) {
      const bytes = fixture
        ? await readFile(resolve('tests/fixtures/generated', fixture))
        : Buffer.from(
            Array.from(
              { length: 10000 },
              (_, i) =>
                `${i + 1}\n00:00:01,000 --> 00:00:05,000\n{\\an8\\k20}<i>Été 世界 ${i + 1}</i>\n\n`,
            ).join(''),
          );
      assert.equal(hash(bytes), digest);
      const original = bytes.toString('utf8'),
        source = join(root, `reference-${kind}.${format}`),
        target = join(root, `target-${kind}.${format}`),
        project = join(root, `translation-${kind}.vsfproject`),
        recovered = join(root, `recovered-${kind}.${format}`),
        recoveredProject = join(root, `recovered-${kind}.vsfproject`);
      await writeFile(source, bytes);
      browser = await restart([
        { kind: 'subtitles', path: source },
        { kind: 'saveSubtitles', path: source },
        { kind: 'saveSubtitles', path: target },
        { kind: 'saveProject', path: project },
        { kind: 'project', path: project },
      ]);
      await install(browser);
      const result = {
        kind,
        format,
        sourceSha256: digest,
        inputBytes: bytes.length,
        cycles: [],
        timings: {},
        pages: [],
        memory: [],
      };
      report.workloads.push(result);
      memory = await sampleMemory(
        getPid(),
        'target-reference-history-project-checkpoint',
      );
      result.memory.push(memory.report);
      await save();
      console.log(`Translation 10k ${kind}: copy, full paging and editing`);
      Object.assign(
        result.timings,
        await browser.execute(
          async (source, dense) => {
            const b = window.__translationBench;
            await b.menu('translationReference');
            await b.wait(b.panel);
            const at = performance.now();
            await b.click('Start target copy…');
            await b.frame();
            const copyStartMs = performance.now() - at;
            await b.wait(async () => {
              const d = await b.state();
              return (
                d.requiresSaveAs &&
                d.cues.length === 10000 &&
                d.translation?.reference?.path === source &&
                b.ready()
              );
            });
            await b.page(1, dense ? 100 : 1);
            await b.frame();
            const copyPreparationMs = performance.now() - at;
            if (!dense) {
              const select = document.querySelector(
                '[aria-label="Reference view"]',
              );
              select.selectedIndex = 1;
              select.dispatchEvent(new Event('change', { bubbles: true }));
            }
            await b.page(1, 100);
            const before = await b.state();
            await b.menu('saveAs');
            await b.wait(
              () =>
                b.ready() &&
                /original|reference|source/i.test(
                  document.querySelector('footer [role="status"]')
                    ?.textContent ?? '',
                ),
            );
            if (JSON.stringify(await b.state()) !== JSON.stringify(before))
              throw new Error('Protected source save changed document');
            return { copyStartMs, copyPreparationMs };
          },
          source,
          kind === 'dense',
        ),
      );
      const baseline = await status();
      assert.equal(baseline.cues.length, 10000);
      assert.equal(baseline.translation.reference.sha256, digest);
      assert.equal(baseline.format?.name ?? 'srt', format);
      assert.deepEqual(await readFile(source), bytes);
      result.timings.initialSaveMs = await browser.execute(async (target) => {
        const b = window.__translationBench,
          at = performance.now();
        await b.menu('saveAs');
        await b.wait(async () => {
          const d = await b.state();
          return (
            d.path === target && !d.dirty && !d.requiresSaveAs && b.ready()
          );
        });
        await b.frame();
        return performance.now() - at;
      }, target);
      assert.deepEqual(await readFile(target), bytes);
      // All 100 pages are visited; dense workloads use matching, others browsing.
      // Keep each transport call below its 10s timeout even when several
      // individual pages approach the unchanged 1s interaction limit.
      for (let batch = 0; batch < 20; batch++) {
        result.pages.push(
          ...(await browser.execute(async (start) => {
            const b = window.__translationBench,
              pages = [];
            for (let page = start; page < start + 5; page++) {
              const at = performance.now();
              if (page > 0) await b.click('Next reference page');
              await b.page(page * 100 + 1, 100);
              await b.frame();
              const indices = b.rows().map((r) =>
                Number(
                  r
                    .querySelector('strong')
                    .textContent.trim()
                    .match(/^#(\d+)/)[1],
                ),
              );
              if (indices.some((n, i) => n !== page * 100 + i + 1))
                throw new Error('Reference page lost/reordered cues');
              pages.push({
                first: indices[0],
                last: indices.at(-1),
                pageMs: performance.now() - at,
              });
            }
            return pages;
          }, batch * 5)),
        );
        await save();
      }
      // Switching away and back intentionally resets the page without changing data.
      await browser.execute(async () => {
        const b = window.__translationBench;
        await b.click('Close reference panel');
        await b.wait(() => !b.panel());
        await b.menu('translationReference');
        await b.wait(b.panel);
        const select = document.querySelector('[aria-label="Reference view"]');
        select.selectedIndex = 1;
        select.dispatchEvent(new Event('change', { bubbles: true }));
        await b.page(1, 100);
      });
      const newline =
        kind === 'styled' ? '\r\n' : kind === 'timed' ? '\r' : '\n';
      const rawFirst = baseline.cues[0].text.replaceAll('\n', newline);
      assert(original.includes(rawFirst));
      let expectedSaved = original;
      for (let cycle = 0; cycle < 5; cycle++) {
        const text = `Translated ${cycle} ${baseline.cues[0].text}`,
          offset = cycle % 2 ? 1500 : -1500;
        const times = await browser.execute(
          async (text, offset) => {
            const b = window.__translationBench,
              before = await b.state();
            b.input('Subtitle text', text);
            await b.frame();
            const offsetMs = await b.offset(offset);
            if (
              document.querySelector('[aria-label="Subtitle text"]').value !==
                text ||
              (await b.state()).revision !== before.revision
            )
              throw new Error('Offset changed target draft/revision');
            const at = performance.now();
            b.rows()[0].click();
            const selected = await b.wait(() =>
              document.querySelector('[aria-label="Selected reference cue"]'),
            );
            if (
              selected.querySelectorAll('pre')[1]?.textContent !==
              before.cues[0].text
            )
              throw new Error('Raw reference changed with target edit');
            await b.frame();
            const selectedReferenceMs = performance.now() - at;
            const editAt = performance.now();
            await b.click('Apply edit');
            const accepted = await b.wait(async () => {
              const d = await b.state();
              return d.revision === before.revision + 1 && b.ready() && d;
            });
            await b.page(1, 100);
            await b.frame();
            const editMs = performance.now() - editAt;
            if (accepted.cues[0].text !== text)
              throw new Error('Target edit missing');
            const undoMs = await b.history('Undo edit', accepted.revision);
            if (
              JSON.stringify((await b.state()).cues) !==
              JSON.stringify(before.cues)
            )
              throw new Error('Translation Undo lost cues');
            const redoMs = await b.history('Redo edit', accepted.revision + 1);
            if (
              JSON.stringify((await b.state()).cues) !==
              JSON.stringify(accepted.cues)
            )
              throw new Error('Translation Redo lost cues');
            const saveAt = performance.now();
            await b.menu('save');
            await b.wait(async () => !(await b.state()).dirty && b.ready());
            await b.frame();
            const saveMs = performance.now() - saveAt;
            const projectAt = performance.now();
            await b.menu('saveProject');
            await b.wait(
              async () =>
                !(await b.state()).translation.dirty &&
                b.ready() &&
                document.querySelector('footer [role="status"]')
                  ?.textContent === 'Project saved',
            );
            await b.frame();
            const projectSaveMs = performance.now() - projectAt;
            const restoreMs = await b.history(
              'Undo edit',
              accepted.revision + 2,
            );
            return {
              offsetMs,
              selectedReferenceMs,
              editMs,
              undoMs,
              redoMs,
              saveMs,
              projectSaveMs,
              restoreMs,
            };
          },
          text,
          offset,
        );
        result.cycles.push(times);
        assert.deepEqual((await status()).cues, baseline.cues);
        expectedSaved = original.replace(
          rawFirst,
          text.replaceAll('\n', newline),
        );
        assert.equal(await readFile(target, 'utf8'), expectedSaved);
        assert.deepEqual(await readFile(source), bytes);
        const data = JSON.parse(await readFile(project, 'utf8'));
        assert.equal(data.translation.reference.sha256, digest);
        assert.equal(data.translation.reference.offsetMs, offset);
        assert.equal(data.mediaPath, null);
        await save();
      }
      await browser.execute(async () => {
        const b = window.__translationBench;
        await b.history('Redo edit', (await b.state()).revision);
      });
      const saved = await status();
      result.timings.projectReopenMs = await browser.execute(async (target) => {
        const b = window.__translationBench,
          before = await b.state(),
          at = performance.now();
        await b.menu('openProject');
        await b.wait(async () => {
          const d = await b.state();
          return (
            d.path === target &&
            d.generation > before.generation &&
            d.cues.length === 10000 &&
            b.ready()
          );
        });
        await b.frame();
        return performance.now() - at;
      }, target);
      assert.deepEqual((await status()).cues, saved.cues);
      assert.equal((await status()).translation.reference.sha256, digest);
      // No UI query competes with these direct cancellation/stale-owner probes.
      const guards = await browser.execute(async (source) => {
        const b = window.__translationBench;
        if (b.panel()) {
          await b.click('Close reference panel');
          await b.wait(() => !b.panel());
        }
        const old = await b.translation({ kind: 'status' });
        if (old.job) {
          await b.translation({ kind: 'cancel', id: old.job.id });
          await b.finish(old.job.id);
        }
        const before = await b.state(),
          key = b.key(before);
        const load = await b.translation({
          kind: 'load',
          key,
          path: source,
          copy: false,
        });
        const at = performance.now();
        await b.translation({ kind: 'cancel', id: load.job.id });
        await b.frame();
        const cancellationMs = performance.now() - at;
        const cancelled = await b.finish(load.job.id),
          cancellationDrainedMs = performance.now() - at;
        if (cancelled.job.phase !== 'cancelled')
          throw new Error('Load cancellation did not drain');
        let refused = false;
        try {
          await b.translation({
            kind: 'accept',
            id: load.job.id,
            discard: false,
          });
        } catch {
          refused = true;
        }
        if (
          !refused ||
          JSON.stringify(await b.state()) !== JSON.stringify(before)
        )
          throw new Error('Cancelled load adopted state');
        const next = await b.translation({
          kind: 'load',
          key,
          path: source,
          copy: false,
        });
        if ((await b.finish(next.job.id)).job.phase !== 'ready')
          throw new Error('Reference preparation failed');
        b.input('Subtitle text', `Stale guard ${before.cues[0].text}`);
        await b.click('Apply edit');
        await b.wait(
          async () =>
            (await b.state()).revision === before.revision + 1 && b.ready(),
        );
        const staleAt = performance.now();
        refused = false;
        try {
          await b.translation({
            kind: 'accept',
            id: next.job.id,
            discard: false,
          });
        } catch {
          refused = true;
        }
        const staleRejectionMs = performance.now() - staleAt;
        if (!refused) throw new Error('Stale reference load adopted');
        await b.history('Undo edit', before.revision + 1);
        if (
          JSON.stringify((await b.state()).cues) !== JSON.stringify(before.cues)
        )
          throw new Error('Stale-guard Undo lost state');
        return { cancellationMs, cancellationDrainedMs, staleRejectionMs };
      }, source);
      Object.assign(result.timings, guards);
      const recoveryText = `Recovered ${baseline.cues[0].text}`;
      const expectedRecovery = original.replace(
        rawFirst,
        recoveryText.replaceAll('\n', newline),
      );
      result.timings.recoveryEditMs = await browser.execute(async (text) => {
        const b = window.__translationBench,
          before = await b.state();
        b.input('Subtitle text', text);
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
      await browser.execute(async () => {
        const b = window.__translationBench;
        await b.menu('translationReference');
        await b.wait(b.panel);
        const select = document.querySelector('[aria-label="Reference view"]');
        select.selectedIndex = 1;
        select.dispatchEvent(new Event('change', { bubbles: true }));
        await b.page(1, 100);
        await b.offset(-3000);
      });
      const accepted = await status();
      await until(
        'exact translation 10k target/reference recovery checkpoint',
        async () => {
          for (const entry of await readdir(join(root, 'recovery-v1'), {
            withFileTypes: true,
          })) {
            if (!entry.isDirectory()) continue;
            try {
              const p = join(root, 'recovery-v1', entry.name, 'snapshot.json');
              const bytes = await readFile(p);
              const snapshot = JSON.parse(bytes.toString('utf8'));
              if (
                snapshot.source_path === target &&
                snapshot.generation === accepted.generation &&
                snapshot.revision === accepted.revision &&
                snapshot.accepted_srt === expectedRecovery &&
                snapshot.translation?.reference?.text === original &&
                snapshot.translation.reference.offset_ms === -3000
              ) {
                result.checkpointBytes = bytes.length;
                return true;
              }
            } catch (e) {
              if (e.code !== 'ENOENT') throw e;
            }
          }
          return false;
        },
      );
      if (report.diagnosticDrawProfile)
        memory.report.draws = await browser.execute(
          () => window.__translationDraws,
        );
      await memory.stop();
      memory = null;
      await save();
      await unlink(source);
      const external = original.replace(rawFirst, `External ${rawFirst}`);
      await writeFile(target, external);
      browser = await restart([
        { kind: 'saveSubtitles', path: source },
        { kind: 'saveSubtitles', path: target },
        { kind: 'saveSubtitles', path: recovered },
        { kind: 'saveProject', path: recoveredProject },
        { kind: 'saveSubtitles', path: source },
        { kind: 'subtitles', path: source },
      ]);
      await install(browser);
      memory = await sampleMemory(
        getPid(),
        'recovery-reference-browse-save-project',
      );
      result.memory.push(memory.report);
      console.log(`Translation 10k ${kind}: captured-reference recovery`);
      result.timings.recoveryOpenMs = await browser.execute(async (target) => {
        const b = window.__translationBench;
        const row = await b.wait(() =>
          [...document.querySelectorAll('.recovery-row')].find(
            (r) => r.querySelector('span')?.title === target,
          ),
        );
        const at = performance.now();
        [...row.querySelectorAll('button')]
          .find((e) => e.textContent.trim() === 'Recover')
          .click();
        await b.wait(async () => {
          const d = await b.state();
          return (
            d.requiresSaveAs &&
            d.cues.length === 10000 &&
            d.translation?.reference?.cues === 10000 &&
            b.ready()
          );
        });
        await b.frame();
        return performance.now() - at;
      }, target);
      const restored = await status();
      assert.deepEqual(restored.cues, accepted.cues);
      assert.equal(restored.translation.reference.sha256, digest);
      assert.equal(restored.translation.reference.offsetMs, -3000);
      assert.equal(restored.canUndo, false);
      Object.assign(
        result.timings,
        await browser.execute(async () => {
          const b = window.__translationBench;
          const at = performance.now();
          await b.menu('translationReference');
          await b.wait(b.panel);
          const select = document.querySelector(
            '[aria-label="Reference view"]',
          );
          select.selectedIndex = 1;
          select.dispatchEvent(new Event('change', { bubbles: true }));
          await b.page(1, 100);
          await b.frame();
          const recoveryBrowseMs = performance.now() - at;
          await b.menu('saveAs');
          await b.wait(
            () =>
              b.ready() &&
              /original|reference|source/i.test(
                document.querySelector('footer [role="status"]')?.textContent ??
                  '',
              ),
          );
          await b.menu('saveAs');
          await b.wait(
            () =>
              b.ready() &&
              /changed on disk/i.test(
                document.querySelector('footer [role="status"]')?.textContent ??
                  '',
              ),
          );
          return { recoveryBrowseMs };
        }),
      );
      await assert.rejects(readFile(source), { code: 'ENOENT' });
      assert.equal(await readFile(target, 'utf8'), external);
      result.timings.recoverySaveMs = await browser.execute(
        async (recovered) => {
          const b = window.__translationBench,
            at = performance.now();
          await b.menu('saveAs');
          await b.wait(async () => {
            const d = await b.state();
            return (
              d.path === recovered && !d.requiresSaveAs && !d.dirty && b.ready()
            );
          });
          await b.frame();
          return performance.now() - at;
        },
        recovered,
      );
      assert.equal(await readFile(recovered, 'utf8'), expectedRecovery);
      await writeFile(source, bytes);
      result.timings.recoveryProjectMs = await browser.execute(async () => {
        const b = window.__translationBench,
          at = performance.now();
        await b.menu('saveProject');
        await b.wait(
          async () =>
            !(await b.state()).translation.dirty &&
            b.ready() &&
            document.querySelector('footer [role="status"]')?.textContent ===
              'Project saved',
        );
        await b.frame();
        return performance.now() - at;
      });
      const recoveredData = JSON.parse(
        await readFile(recoveredProject, 'utf8'),
      );
      assert.equal(recoveredData.translation.reference.sha256, digest);
      assert.equal(recoveredData.translation.reference.offsetMs, -3000);
      Object.assign(
        result.timings,
        await browser.execute(async (digest) => {
          const b = window.__translationBench,
            before = await b.state();
          const draft = `Unapplied detach draft ${before.cues[0].text}`;
          b.input('Subtitle text', draft);
          await b.frame();
          let at = performance.now();
          await b.click('Detach reference');
          await b.wait(
            async () => !(await b.state()).translation?.reference && b.ready(),
          );
          await b.frame();
          const detachMs = performance.now() - at;
          const detached = await b.state();
          if (
            detached.revision !== before.revision ||
            JSON.stringify(detached.cues) !== JSON.stringify(before.cues) ||
            document.querySelector('[aria-label="Subtitle text"]').value !==
              draft
          )
            throw new Error('Detachment changed target or draft');
          await b.click('Revert draft');
          await b.menu('saveAs');
          await b.wait(
            () =>
              b.ready() &&
              /original|reference|source/i.test(
                document.querySelector('footer [role="status"]')?.textContent ??
                  '',
              ),
          );
          if ((await b.state()).path !== before.path)
            throw new Error('Detached copied origin was overwritten');
          at = performance.now();
          await b.click('Attach reference…');
          await b.frame();
          const attachStartMs = performance.now() - at;
          await b.wait(
            async () =>
              (await b.state()).translation?.reference?.sha256 === digest &&
              b.ready(),
          );
          await b.page(1, 100);
          await b.frame();
          const attachPreparationMs = performance.now() - at;
          if ((await b.state()).revision !== before.revision)
            throw new Error('Reattachment changed target revision');
          await b.menu('saveProject');
          await b.wait(
            async () => !(await b.state()).translation.dirty && b.ready(),
          );
          return { detachMs, attachStartMs, attachPreparationMs };
        }, digest),
      );
      assert.deepEqual(await readFile(source), bytes);
      assert.equal(await readFile(recovered, 'utf8'), expectedRecovery);
      assert.equal(
        JSON.parse(await readFile(recoveredProject, 'utf8')).translation
          .reference.offsetMs,
        0,
      );
      await browser.saveScreenshot(
        join(evidence, `translation-${kind}-10000.png`),
      );
      result.visibleTargetRows = await browser.execute(
        () => document.querySelectorAll('.cue-row').length,
      );
      assert(result.visibleTargetRows > 0 && result.visibleTargetRows <= 200);
      if (report.diagnosticDrawProfile)
        memory.report.draws = await browser.execute(
          () => window.__translationDraws,
        );
      await memory.stop();
      memory = null;
      result.outputSha256 = hash(expectedRecovery);
      result.violations = [];
      for (const times of [
        result.timings,
        ...result.cycles,
        ...result.pages.map(({ pageMs }) => ({ pageMs })),
      ]) {
        for (const [name, ms] of Object.entries(times)) {
          const budget =
            name === 'copyPreparationMs' || name === 'attachPreparationMs'
              ? report.preparationDeadlineMs
              : report.budgetMs;
          if (ms >= budget)
            result.violations.push(`${name}: ${ms}ms >= ${budget}ms`);
        }
      }
      result.status = result.violations.length ? 'failed' : 'passed';
      await save();
      await record({
        step: `translation 10k ${kind} combined paging/history/project/recovery/cancel/stale`,
        status: result.status,
        violations: result.violations,
      });
    }
    assert.deepEqual(
      report.workloads.flatMap((r) =>
        r.violations.map((v) => `${r.kind} ${v}`),
      ),
      [],
      'Translation desktop performance gates failed',
    );
    report.status = 'passed';
  } catch (error) {
    report.status = 'failed';
    report.error = String(error.stack ?? error);
    throw error;
  } finally {
    try {
      if (memory) {
        if (report.diagnosticDrawProfile)
          memory.report.draws = await browser
            .execute(() => window.__translationDraws)
            .catch(() => null);
        await memory.stop();
      }
    } finally {
      await save();
    }
  }
}
