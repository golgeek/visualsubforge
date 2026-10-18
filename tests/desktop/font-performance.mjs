// Native webview + Rust on the established 10k corpus with a small original font.
// Preparation is recorded separately; DOM/IPC times are not physical input times.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { copyFile, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
export async function fontPerformance({
  restart,
  root,
  evidence,
  binary,
  record,
}) {
  const report = {
    status: 'running',
    binary,
    binarySha256: hash(await readFile(binary)),
    budgetMs: 1000,
    scope:
      'Actual webview/Rust with generated DOM/IPC. Representative small fonts, exact original 10k corpus and separately recorded attached-input hashes. No physical-input or GPU presentation latency claim.',
    cases: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'font-performance.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const first = join(root, 'Fixture.ttf'),
    second = join(root, 'Changed.otf');
  await copyFile(resolve('tests/fixtures/subtitles/fixture-font.ttf'), first);
  await copyFile(
    resolve('tests/fixtures/subtitles/fixture-font-cff.otf'),
    second,
  );
  // Use the checked-in original fixture's canonical attachment payload.
  const embedded = await readFile(
    resolve('tests/fixtures/subtitles/embedded-font.ass'),
    'utf8',
  );
  const payload = embedded
    .slice(embedded.indexOf('[Fonts]'))
    .replace('fontname: VSFFixtureFont.ttf', 'fontname: Fixture.ttf');
  assert.ok(payload.startsWith('[Fonts]') && payload.includes('fontname:'));
  try {
    for (const extension of ['ass', 'ssa']) {
      const corpus = await readFile(
        resolve(`tests/fixtures/generated/ass-ssa/10000.${extension}`),
      );
      for (const operation of ['add', 'replace', 'remove']) {
        const bytes =
          operation === 'add'
            ? corpus
            : Buffer.from(corpus.toString('utf8') + '\n' + payload);
        const path = join(root, `font-${operation}-10000.${extension}`);
        await writeFile(path, bytes);
        const browser = await restart([
          { kind: 'subtitles', path },
          ...Array.from({ length: operation === 'remove' ? 0 : 5 }, () => ({
            kind: 'font',
            path: operation === 'add' ? first : second,
          })),
        ]);
        await browser.execute(() => {
          const invoke = window.__TAURI__.core.invoke;
          window.__fontBench = {
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
                (b) => b.textContent.trim() === label && !b.disabled,
              ),
            wait: async (probe) => {
              const end = performance.now() + 12000;
              while (performance.now() < end) {
                const result = await probe();
                if (result) return result;
                await new Promise((r) => setTimeout(r, 5));
              }
              throw new Error('Font benchmark did not settle');
            },
            ready: () =>
              document.querySelector('main').getAttribute('aria-busy') ===
                'false' && !document.querySelector('dialog[open]'),
          };
        });
        const openMs = await browser.execute(async () => {
          const b = window.__fontBench,
            began = performance.now();
          await window.__TAURI__.event.emit(
            'workspace-action',
            'openSubtitles',
          );
          await b.wait(
            async () => (await b.state()).cues.length === 10000 && b.ready(),
          );
          await b.frame();
          return performance.now() - began;
        });
        const result = {
          extension,
          operation,
          cues: 10000,
          corpusSha256: hash(corpus),
          inputSha256: hash(bytes),
          openMs,
          inventoryMs: [],
          preparationMs: [],
          acceptMs: [],
          undoMs: [],
        };
        report.cases.push(result);
        await save();
        for (let run = 0; run < 5; run++) {
          const measured = await browser.execute(async (operation) => {
            const b = window.__fontBench;
            await b.wait(b.ready);
            const inspectAt = performance.now();
            b.button('Manage fonts…').click();
            await b.wait(() =>
              document
                .querySelector(
                  'dialog[aria-labelledby="font-dialog-title"][open]',
                )
                ?.textContent.includes('Font review: reviewing'),
            );
            await b.frame();
            const inventoryMs = performance.now() - inspectAt;
            const before = await b.state(),
              began = performance.now();
            b.button(
              operation === 'add'
                ? 'Add font…'
                : operation === 'replace'
                  ? 'Replace font…'
                  : 'Preview removal',
            ).click();
            await b.wait(() => b.button('Accept font change'));
            await b.frame();
            const preparationMs = performance.now() - began;
            const acceptedAt = performance.now();
            b.button('Accept font change').click();
            await b.wait(
              async () =>
                (await b.state()).revision === before.revision + 1 && b.ready(),
            );
            await b.frame();
            const acceptMs = performance.now() - acceptedAt;
            const after = await b.state();
            if (JSON.stringify(after.cues) !== JSON.stringify(before.cues))
              throw new Error('Font change mutated cues');
            const undoAt = performance.now();
            b.button('Undo edit').click();
            await b.wait(
              async () =>
                (await b.state()).revision === before.revision + 2 && b.ready(),
            );
            await b.frame();
            const undoMs = performance.now() - undoAt;
            if ((await b.state()).dirty)
              throw new Error('Undo did not restore saved source');
            return { inventoryMs, preparationMs, acceptMs, undoMs };
          }, operation);
          for (const [key, value] of Object.entries(measured))
            result[key].push(value);
        }
        const guards = await browser.execute(async (path) => {
          const b = window.__fontBench,
            doc = await b.state();
          const request = {
            generation: doc.generation,
            revision: doc.revision,
            operation: { kind: 'add', path },
          };
          const id = await b.invoke('font_start', request),
            began = performance.now();
          await b.invoke('font_cancel', { id });
          await b.frame();
          const cancelMs = performance.now() - began;
          await b.wait(
            async () =>
              (await b.invoke('font_view', { id })).phase === 'cancelled',
          );
          let refused = false;
          try {
            await b.invoke('font_accept', { id });
          } catch {
            refused = true;
          }
          if (!refused) throw new Error('Cancelled font accepted');
          const stale = await b.invoke('font_start', request);
          await b.wait(async () =>
            ['reviewing'].includes(
              (await b.invoke('font_view', { id: stale })).phase,
            ),
          );
          const cue = doc.cues[0],
            at = performance.now();
          const field = document.querySelector('[aria-label="Subtitle text"]');
          field.value = cue.text + ' stale';
          field.dispatchEvent(new Event('input', { bubbles: true }));
          await b.frame();
          b.button('Apply edit').click();
          await b.wait(
            async () =>
              (await b.state()).revision === doc.revision + 1 && b.ready(),
          );
          if ((await b.invoke('font_view', { id: stale })).phase !== 'stale')
            throw new Error('Edited context kept font review current');
          refused = false;
          try {
            await b.invoke('font_accept', { id: stale });
          } catch {
            refused = true;
          }
          if (!refused) throw new Error('Stale font accepted');
          await b.frame();
          const staleMs = performance.now() - at;
          b.button('Undo edit').click();
          await b.wait(
            async () =>
              (await b.state()).revision === doc.revision + 2 && b.ready(),
          );
          if ((await b.state()).dirty)
            throw new Error('Guard cleanup did not restore saved state');
          return { cancelMs, staleMs };
        }, second);
        Object.assign(result, guards);
        for (const [name, values] of Object.entries({
          openMs: [result.openMs],
          inventoryMs: result.inventoryMs,
          acceptMs: result.acceptMs,
          undoMs: result.undoMs,
          cancelMs: [result.cancelMs],
          staleMs: [result.staleMs],
        }))
          assert.ok(
            values.every((v) => v < 1000),
            `${extension}/${operation} ${name} exceeded 1000ms: ${values}`,
          );
        assert.ok(result.preparationMs.every((v) => v < 10000));
        await save();
        await record({
          step: `${extension} 10k font ${operation} responsiveness`,
          result,
        });
      }
    }
    report.status = 'passed';
    await save();
  } catch (e) {
    report.status = 'failed';
    report.error = String(e.stack ?? e);
    await save();
    throw e;
  }
}
