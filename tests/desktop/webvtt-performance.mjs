// Exact library fixtures through actual WebView/Rust/files. Generated DOM events
// and supplied file choices measure application latency, not physical input.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, readdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const hashes = {
  plain: '337cffd83d4ad10fc053bd565f96b4cbc77fd7c817db39fc40e6d254a5d09198',
  styled: '783a7ad89406073e0cb6512c9ca9daaac9b7c09baeaa933dada2bd0e67aa339f',
  timed: '46430c8e7b374e2bd6ccb3d4bd95cf3489d2571f00ff8c9120c9c59ce5e1c1e6',
};

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
      frame: () => new Promise((r) => requestAnimationFrame(r)),
      ready: () =>
        document.querySelector('main')?.getAttribute('aria-busy') === 'false' &&
        !document.querySelector('dialog[open]'),
      button: (label) =>
        [...document.querySelectorAll('button')].find(
          (b) => b.textContent.trim() === label,
        ),
      input: (text) => {
        const e = document.querySelector('[aria-label="Subtitle text"]');
        e.value = text;
        e.dispatchEvent(new Event('input', { bubbles: true }));
      },
      menu: (action) => window.__TAURI__.event.emit('workspace-action', action),
      wait: async (probe, timeout = 4000) => {
        const end = performance.now() + timeout;
        while (performance.now() < end) {
          const value = await probe();
          if (value) return value;
          await new Promise((r) => setTimeout(r, 10));
        }
        throw new Error('WebVTT desktop measurement did not settle');
      },
    };
    b.click = async (label) => {
      await b.wait(() => b.button(label) && !b.button(label).disabled);
      b.button(label).click();
    };
    b.history = async (label, revision) => {
      const start = performance.now();
      await b.click(label);
      await b.wait(
        async () => (await b.state()).revision === revision + 1 && b.ready(),
      );
      await b.frame();
      return performance.now() - start;
    };
    window.__webvttBench = b;
  });
}

export async function webvttPerformance({
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
      'Actual WebView/Rust/files; five edit/history/save cycles per exact 10k input, full reopen/recovery and cancellation/stale checks. Generated DOM input and supplied file choices. No native media loaded. Pure CPU/RSS are recorded separately in webvtt.md.',
    workloads: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'webvtt-performance.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  try {
    for (const [kind, digest] of Object.entries(hashes)) {
      const bytes = await readFile(
        resolve(`tests/fixtures/generated/webvtt/10000-${kind}.vtt`),
      );
      assert.equal(hash(bytes), digest);
      const original = bytes.toString('utf8'),
        source = join(root, `${kind}.vtt`),
        copy = join(root, `${kind}-copy.vtt`),
        recovered = join(root, `${kind}-recovered.vtt`);
      await writeFile(source, bytes);
      let browser = await restart([
        { kind: 'subtitles', path: source },
        { kind: 'saveSubtitles', path: copy },
        { kind: 'subtitles', path: copy },
      ]);
      await install(browser);
      const result = {
        kind,
        sourceSha256: digest,
        inputBytes: bytes.length,
        cycles: [],
        timings: {},
      };
      report.workloads.push(result);
      await save();
      result.timings.openMs = await browser.execute(async () => {
        const b = window.__webvttBench,
          start = performance.now();
        await b.menu('openSubtitles');
        await b.wait(
          async () =>
            (await b.state()).cues.length === 10000 &&
            b.ready() &&
            document.querySelector('.cue-row'),
        );
        await b.frame();
        return performance.now() - start;
      });
      const baseline = await status();
      assert.equal(baseline.format.name, 'vtt');
      assert.equal(baseline.format.timeStepMs, 1);
      assert.equal(baseline.cues.length, 10000);
      const newline =
        kind === 'styled' ? '\r\n' : kind === 'timed' ? '\r' : '\n';
      const rawFirst = baseline.cues[0].text.replaceAll('\n', newline);
      assert(original.includes(rawFirst));
      let expectedSaved;
      for (let cycle = 0; cycle < 5; cycle++) {
        const text = `${baseline.cues[0].text}!${cycle}`;
        const times = await browser.execute(
          async (text, cycle, copy) => {
            const b = window.__webvttBench,
              before = await b.state();
            let start = performance.now();
            b.input(text);
            await b.frame();
            const draftMs = performance.now() - start;
            start = performance.now();
            await b.click('Apply edit');
            const accepted = await b.wait(async () => {
              const d = await b.state();
              return d.revision === before.revision + 1 && b.ready() && d;
            });
            await b.frame();
            const acceptMs = performance.now() - start;
            if (accepted.cues[0].text !== text)
              throw new Error('WebVTT edit was not accepted');
            const undoMs = await b.history('Undo edit', accepted.revision);
            if (
              JSON.stringify((await b.state()).cues) !==
              JSON.stringify(before.cues)
            )
              throw new Error('WebVTT Undo lost cue state');
            const redoMs = await b.history('Redo edit', accepted.revision + 1);
            if (
              JSON.stringify((await b.state()).cues) !==
              JSON.stringify(accepted.cues)
            )
              throw new Error('WebVTT Redo lost cue state');
            start = performance.now();
            await b.menu(cycle === 0 ? 'saveAs' : 'save');
            await b.wait(async () => {
              const d = await b.state();
              return !d.dirty && d.path === copy && b.ready();
            });
            await b.frame();
            const saveMs = performance.now() - start;
            const restoreMs = await b.history(
              'Undo edit',
              accepted.revision + 2,
            );
            return { draftMs, acceptMs, undoMs, redoMs, saveMs, restoreMs };
          },
          text,
          cycle,
          copy,
        );
        result.cycles.push(times);
        assert.deepEqual((await status()).cues, baseline.cues);
        expectedSaved = original.replace(rawFirst, `${rawFirst}!${cycle}`);
        assert.equal(await readFile(copy, 'utf8'), expectedSaved);
        assert.deepEqual(await readFile(source), bytes);
        await save();
      }
      await browser.execute(async () => {
        const b = window.__webvttBench;
        await b.history('Redo edit', (await b.state()).revision);
      });
      const saved = await status();
      assert.equal(saved.dirty, false);
      result.timings.reopenMs = await browser.execute(async (copy) => {
        const b = window.__webvttBench;
        await b.menu('closeSubtitles');
        await b.wait(async () => !(await b.state()).loaded && b.ready());
        const start = performance.now();
        await b.menu('openSubtitles');
        await b.wait(async () => {
          const d = await b.state();
          return d.path === copy && d.cues.length === 10000 && b.ready();
        });
        await b.frame();
        return performance.now() - start;
      }, copy);
      const reopened = await status();
      assert.deepEqual(reopened.cues, saved.cues);
      const asyncResult = await browser.execute(
        async (query) => {
          const b = window.__webvttBench,
            before = await b.state();
          const start = () =>
            b.invoke('text_start', {
              generation: before.generation,
              revision: before.revision,
              operation: { kind: 'find', query, caseSensitive: true },
            });
          const view = (id) =>
            b.invoke('text_view', { id, offset: 0, limit: 200 });
          const id = await start();
          let began = performance.now();
          b.input(`${before.cues[0].text} draft`);
          await b.frame();
          const typingMs = performance.now() - began;
          began = performance.now();
          await b.invoke('text_cancel', { id });
          await b.wait(async () => (await view(id)).phase === 'cancelled');
          const cancellationMs = performance.now() - began;
          await b.click('Revert draft');
          await b.frame();
          began = performance.now();
          const next = await start();
          const prepared = await b.wait(async () => {
            const v = await view(next);
            if (v.phase === 'failed') throw new Error(v.error);
            return v.phase === 'reviewing' && v;
          }, 12000);
          const preparationMs = performance.now() - began;
          b.input(`${before.cues[0].text} stale`);
          await b.frame();
          began = performance.now();
          await b.click('Apply edit');
          await b.wait(async () => {
            const v = await view(next);
            return v.phase === 'stale' && v.rows.length === 0 && b.ready();
          });
          const invalidationMs = performance.now() - began;
          let rejected = false;
          try {
            await b.invoke('text_accept', { id: next, target: null });
          } catch {
            rejected = true;
          }
          if (!rejected || (await b.state()).revision !== before.revision + 1)
            throw new Error('Stale WebVTT proposal was not rejected');
          await b.history('Undo edit', before.revision + 1);
          if (
            JSON.stringify((await b.state()).cues) !==
            JSON.stringify(before.cues)
          )
            throw new Error('Stale-guard Undo lost state');
          return {
            typingMs,
            cancellationMs,
            invalidationMs,
            preparationMs,
            pageRows: prepared.rows.length,
            matches: prepared.matches,
          };
        },
        kind === 'plain' ? 'Bonjour' : 'Été',
      );
      assert.equal(asyncResult.pageRows, 200);
      assert.equal(asyncResult.matches, 10000);
      result.preparationMs = asyncResult.preparationMs;
      assert(asyncResult.preparationMs < report.preparationDeadlineMs);
      for (const field of ['typingMs', 'cancellationMs', 'invalidationMs'])
        result.timings[field] = asyncResult[field];
      result.pageRows = asyncResult.pageRows;
      assert.deepEqual((await status()).cues, saved.cues);
      result.timings.navigationMs = await browser.execute(async () => {
        const b = window.__webvttBench,
          start = performance.now(),
          list = document.querySelector('.cue-list');
        list.scrollTop = list.scrollHeight;
        list.dispatchEvent(new Event('scroll'));
        const row = await b.wait(() =>
          document.querySelector('[data-cue-id="10000"]'),
        );
        row.click();
        await b.wait(
          () =>
            row.getAttribute('aria-pressed') === 'true' &&
            !b.button('Bold').disabled,
        );
        await b.frame();
        return performance.now() - start;
      });
      result.visibleRows = await browser.execute(
        () => document.querySelectorAll('.cue-row').length,
      );
      assert(result.visibleRows > 0 && result.visibleRows <= 200);
      await browser.saveScreenshot(join(evidence, `webvtt-${kind}-10000.png`));
      const recoveryText = `${saved.cues.at(-1).text} VSF recovery ${kind}`;
      result.timings.recoveryEditMs = await browser.execute(async (text) => {
        const b = window.__webvttBench,
          before = await b.state();
        b.input(text);
        await b.frame();
        const start = performance.now();
        await b.click('Apply edit');
        await b.wait(
          async () =>
            (await b.state()).revision === before.revision + 1 && b.ready(),
        );
        await b.frame();
        return performance.now() - start;
      }, recoveryText);
      const accepted = await status();
      assert.equal(accepted.cues.at(-1).text, recoveryText);
      assert.deepEqual(accepted.cues.slice(0, -1), saved.cues.slice(0, -1));
      const rawLast = saved.cues.at(-1).text.replaceAll('\n', newline);
      const expectedRecovery = expectedSaved.replace(
        rawLast,
        recoveryText.replaceAll('\n', newline),
      );
      await until('exact 10k WebVTT recovery checkpoint', async () => {
        for (const entry of await readdir(join(root, 'recovery-v1'), {
          withFileTypes: true,
        })) {
          if (!entry.isDirectory()) continue;
          try {
            const r = JSON.parse(
              await readFile(
                join(root, 'recovery-v1', entry.name, 'snapshot.json'),
                'utf8',
              ),
            );
            if (r.format === 'vtt' && r.accepted_srt === expectedRecovery)
              return true;
          } catch (e) {
            if (e.code !== 'ENOENT') throw e;
          }
        }
        return false;
      });
      browser = await restart([{ kind: 'saveSubtitles', path: recovered }]);
      await install(browser);
      result.timings.recoveryOpenMs = await browser.execute(async (copy) => {
        const b = window.__webvttBench;
        const row = await b.wait(() =>
          [...document.querySelectorAll('.recovery-row')].find(
            (r) => r.querySelector('span')?.title === copy,
          ),
        );
        const start = performance.now();
        [...row.querySelectorAll('button')]
          .find((button) => button.textContent.trim() === 'Recover')
          .click();
        await b.wait(async () => {
          const d = await b.state();
          return d.requiresSaveAs && d.cues.length === 10000 && b.ready();
        });
        await b.frame();
        return performance.now() - start;
      }, copy);
      const restored = await status();
      assert.equal(restored.format.name, 'vtt');
      assert.equal(restored.canUndo, false);
      assert.deepEqual(restored.cues, accepted.cues);
      result.timings.recoverySaveMs = await browser.execute(async (path) => {
        const b = window.__webvttBench,
          start = performance.now();
        await b.menu('saveAs');
        await b.wait(async () => {
          const d = await b.state();
          return d.path === path && !d.dirty && !d.requiresSaveAs && b.ready();
        });
        await b.frame();
        return performance.now() - start;
      }, recovered);
      assert.equal(await readFile(recovered, 'utf8'), expectedRecovery);
      assert.equal(await readFile(copy, 'utf8'), expectedSaved);
      assert.deepEqual(await readFile(source), bytes);
      result.outputSha256 = hash(expectedRecovery);
      for (const times of [result.timings, ...result.cycles])
        for (const [label, ms] of Object.entries(times))
          assert(
            ms < report.budgetMs,
            `${kind} ${label}: ${ms} ms exceeded ${report.budgetMs} ms`,
          );
      result.status = 'passed';
      await save();
      await record({
        step: `10,000-cue WebVTT ${kind} edit/history/save/reopen/recovery and stale/cancel checks`,
        ...result,
      });
    }
    report.status = 'passed';
  } catch (error) {
    report.status = 'failed';
    report.error = `${String(error)}\n${error.stack ?? ''}`;
    throw error;
  } finally {
    await save();
  }
}
