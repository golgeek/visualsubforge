// Task016's predeclared 1-second desktop gate on the exact pure-format fixtures.
// These are real Svelte events and Rust IPC, without media/native decode loaded.
import assert from 'node:assert/strict';
import { batchPerformance } from './batch-performance.mjs';
import { createHash } from 'node:crypto';
import { copyFile, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

export async function assPerformance({
  restart,
  status,
  evidence,
  root,
  binary,
  record,
  batch = false,
}) {
  const report = {
    schema: 1,
    status: 'running',
    budgetMs: 1000,
    binary,
    binarySha256: createHash('sha256')
      .update(await readFile(binary))
      .digest('hex'),
    scope:
      'Actual Mac WebView and Rust IPC with generated DOM input. Open includes filesystem work; native decode/presentation is measured separately. No physical-input latency or Windows/Linux acceptance claim.',
    formats: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'ass-performance.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  try {
    for (const extension of ['ass', 'ssa']) {
      const source = resolve(
        `tests/fixtures/generated/ass-ssa/10000.${extension}`,
      );
      const path = join(root, `10000.${extension}`);
      const bytes = await readFile(source);
      const hash = createHash('sha256').update(bytes).digest('hex');
      assert.equal(
        hash,
        extension === 'ass'
          ? '00e4deb1210c47e2943c30744da0e5d0512a57ba159b659d9dd16ef5d47c1c0c'
          : '0d2415e1e2d61776e7c9051c3c3c1c10b1c02770e225652b7b0132c57fb79b14',
      );
      await copyFile(source, path);
      const browser = await restart([{ kind: 'subtitles', path }]);
      const result = {
        extension,
        sourceSha256: hash,
        sourceBytes: bytes.length,
        timings: {},
      };
      report.formats.push(result);
      await save();
      await browser.execute(() => {
        const invoke = window.__TAURI__.core.invoke;
        window.__assBench = {
          invoke,
          state: () =>
            invoke('document_action', {
              action: { kind: 'status' },
              generation: 0,
              revision: 0,
            }),
          frame: () => new Promise((r) => requestAnimationFrame(r)),
          button: (label) =>
            [...document.querySelectorAll('button')].find(
              (b) => b.textContent.trim() === label,
            ),
          input: (label, value) => {
            const e = document.querySelector(`[aria-label="${label}"]`);
            if (!e) throw new Error(`Missing ${label}`);
            e.value = value;
            e.dispatchEvent(
              new Event(e.tagName === 'SELECT' ? 'change' : 'input', {
                bubbles: true,
              }),
            );
          },
          wait: async (probe) => {
            const deadline = performance.now() + 4000;
            while (performance.now() < deadline) {
              const value = await probe();
              if (value) return value;
              await new Promise((r) => setTimeout(r, 10));
            }
            throw new Error('ASS desktop measurement did not settle');
          },
          ready: () =>
            document.querySelector('main').getAttribute('aria-busy') ===
            'false',
        };
      });
      result.timings.openMs = await browser.execute(async () => {
        const b = window.__assBench,
          start = performance.now();
        await window.__TAURI__.event.emit('workspace-action', 'openSubtitles');
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
      assert.equal(baseline.format.name, extension);
      assert.equal(baseline.cues.length, 10000);
      for (const [kind, label, value, field, expected] of [
        [
          'text',
          'Subtitle text',
          `${baseline.cues[0].text}!`,
          'text',
          `${baseline.cues[0].text}!`,
        ],
        ['style', 'Cue style', 'Title', 'style', 'Title'],
        ['timing', 'Cue end', '00:00:01,010', 'endMs', 1010],
      ]) {
        const revision = (await status()).revision;
        const measured = await browser.execute(
          async (label, value, revision, field, expected) => {
            const b = window.__assBench,
              began = performance.now();
            b.input(label, value);
            await b.frame();
            const draftMs = performance.now() - began;
            if (b.button('Apply edit').disabled)
              throw new Error('Apply remained disabled');
            const start = performance.now();
            b.button('Apply edit').click();
            await b.wait(
              async () =>
                (await b.state()).revision === revision + 1 && b.ready(),
            );
            await b.frame();
            const acceptedMs = performance.now() - start;
            if ((await b.state()).cues[0][field] !== expected)
              throw new Error(`The ${field} edit was not accepted`);
            const undoStart = performance.now();
            b.button('Undo edit').click();
            await b.wait(
              async () =>
                (await b.state()).revision === revision + 2 && b.ready(),
            );
            await b.frame();
            return {
              draftMs,
              acceptedMs,
              undoMs: performance.now() - undoStart,
            };
          },
          label,
          value,
          revision,
          field,
          expected,
        );
        result.timings[`${kind}DraftMs`] = measured.draftMs;
        result.timings[`${kind}AcceptedMs`] = measured.acceptedMs;
        result.timings[`${kind}UndoMs`] = measured.undoMs;
        assert.deepEqual((await status()).cues, baseline.cues);
        await save();
      }
      result.timings.navigationMs = await browser.execute(async () => {
        const b = window.__assBench,
          start = performance.now();
        const list = document.querySelector('.cue-list');
        list.scrollTop = list.scrollHeight;
        list.dispatchEvent(new Event('scroll'));
        const row = await b.wait(() =>
          [...document.querySelectorAll('.cue-row')].find(
            (r) => r.firstElementChild.textContent === '10000',
          ),
        );
        row.click();
        await b.wait(
          () =>
            row.getAttribute('aria-pressed') === 'true' &&
            document
              .querySelector('[aria-label="Subtitle text"]')
              .value.includes('Cue 10000'),
        );
        await b.frame();
        return performance.now() - start;
      });
      result.visibleCueRows = await browser.execute(
        () => document.querySelectorAll('.cue-row').length,
      );
      assert.ok(result.visibleCueRows > 0 && result.visibleCueRows <= 200);
      await browser.saveScreenshot(
        join(evidence, `${extension}-10000-navigation.png`),
      );
      const current = await status();
      const asyncResults = await browser.execute(
        async (doc) => {
          const b = window.__assBench;
          const start = () =>
            b.invoke('text_start', {
              ...doc,
              operation: { kind: 'find', query: 'line', caseSensitive: false },
            });
          const view = (id) =>
            b.invoke('text_view', { id, offset: 0, limit: 200 });
          const id = await start();
          const draftStart = performance.now();
          const editor = document.querySelector('[aria-label="Subtitle text"]');
          const original = editor.value;
          b.input('Subtitle text', `${original}!`);
          await b.frame();
          const typingMs = performance.now() - draftStart;
          const cancelStart = performance.now();
          await b.invoke('text_cancel', { id });
          const cancelled = await b.wait(async () => {
            const v = await view(id);
            return v.phase === 'cancelled' && v;
          });
          const cancellationMs = performance.now() - cancelStart;
          b.button('Revert draft').click();
          await b.frame();
          const next = await start();
          const prepared = await b.wait(async () => {
            const v = await view(next);
            if (v.phase === 'failed') throw new Error(v.error);
            return v.phase === 'reviewing' && v;
          });
          b.input('Subtitle text', `${original}!`);
          await b.frame();
          const editStart = performance.now();
          b.button('Apply edit').click();
          const stale = await b.wait(async () => {
            const v = await view(next);
            return v.phase === 'stale' && v.rows.length === 0 && b.ready() && v;
          });
          const invalidationMs = performance.now() - editStart;
          const accepted = await b.state();
          if (
            accepted.revision !== doc.revision + 1 ||
            accepted.cues.at(-1).text !== `${original}!`
          )
            throw new Error('Invalidating edit was not accepted');
          b.button('Undo edit').click();
          await b.wait(
            async () =>
              (await b.state()).revision === doc.revision + 2 && b.ready(),
          );
          return {
            typingMs,
            cancellationMs,
            invalidationMs,
            cancelled: cancelled.phase,
            stale: stale.phase,
            pageRows: prepared.rows.length,
          };
        },
        { generation: current.generation, revision: current.revision },
      );
      assert.equal(asyncResults.cancelled, 'cancelled');
      assert.equal(asyncResults.stale, 'stale');
      assert.equal(asyncResults.pageRows, 200);
      Object.assign(result.timings, {
        typingDuringWorkMs: asyncResults.typingMs,
        cancellationMs: asyncResults.cancellationMs,
        invalidationMs: asyncResults.invalidationMs,
      });
      result.proposalPageRows = asyncResults.pageRows;
      assert.deepEqual((await status()).cues, baseline.cues);
      assert.equal((await status()).dirty, false);
      assert.deepEqual(await readFile(path), bytes);
      await save();
      for (const [label, elapsed] of Object.entries(result.timings))
        assert.ok(
          elapsed < report.budgetMs,
          `${extension} ${label}: ${elapsed} ms exceeded ${report.budgetMs} ms`,
        );
      if (batch)
        result.batch = await batchPerformance({
          browser,
          status,
          evidence,
          extension,
        });
      result.status = 'passed';
      await save();
      await record({
        step: `10,000-cue ${extension} desktop interactions`,
        ...result,
      });
    }
    report.status = 'passed';
  } catch (error) {
    report.status = 'failed';
    report.error = String(error.stack ?? error);
    throw error;
  } finally {
    await save();
  }
}
