// Same exact 10k ASS/SSA files as the pure benchmark. DOM/IPC timings include
// a frame after completion, not physical input or GPU presentation latency.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { copyFile, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
export async function stylePerformance({
  restart,
  root,
  evidence,
  binary,
  record,
}) {
  const report = {
    status: 'running',
    budgetMs: 1000,
    binary,
    binarySha256: createHash('sha256')
      .update(await readFile(binary))
      .digest('hex'),
    scope:
      'Native webview and real Rust on exact 10k corpus. Generated DOM/IPC; no physical-input or presentation claim. Background preparation recorded separately from interaction latency.',
    cases: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'style-performance.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  try {
    for (const extension of ['ass', 'ssa']) {
      const source = resolve(
          `tests/fixtures/generated/ass-ssa/10000.${extension}`,
        ),
        path = join(root, `style-10000.${extension}`);
      const bytes = await readFile(source);
      await copyFile(source, path);
      const browser = await restart([{ kind: 'subtitles', path }]);
      await browser.execute(() => {
        const invoke = window.__TAURI__.core.invoke;
        window.__styleBench = {
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
          input: (label, value) => {
            const e = document.querySelector(`[aria-label="${label}"]`);
            if (!e || e.disabled) throw new Error(`Missing ${label}`);
            e.value = value;
            e.dispatchEvent(
              new Event(e.tagName === 'SELECT' ? 'change' : 'input', {
                bubbles: true,
              }),
            );
          },
          wait: async (probe) => {
            const end = performance.now() + 6000;
            while (performance.now() < end) {
              const value = await probe();
              if (value) return value;
              await new Promise((r) => setTimeout(r, 5));
            }
            throw new Error('Style benchmark did not settle');
          },
          ready: () =>
            document.querySelector('main').getAttribute('aria-busy') ===
              'false' && !document.querySelector('dialog[open]'),
        };
      });
      const openMs = await browser.execute(async () => {
        const b = window.__styleBench,
          began = performance.now();
        await window.__TAURI__.event.emit('workspace-action', 'openSubtitles');
        await b.wait(
          async () => (await b.state()).cues.length === 10000 && b.ready(),
        );
        await b.frame();
        return performance.now() - began;
      });
      const result = {
        extension,
        cues: 10000,
        sha256: createHash('sha256').update(bytes).digest('hex'),
        openMs,
        typingMs: [],
        preparationMs: [],
        acceptMs: [],
        undoMs: [],
      };
      report.cases.push(result);
      await save();
      for (let run = 0; run < 5; run++) {
        const measured = await browser.execute(async () => {
          const b = window.__styleBench;
          await b.wait(b.ready);
          b.button('Inspect styles…').click();
          await b.wait(() => document.querySelector('dialog[open]'));
          b.input('Inspect style', 'Title');
          await b.frame();
          b.button('Edit style').click();
          await b.wait(() =>
            document.querySelector('[aria-label="Style Name"]'),
          );
          const typing = performance.now();
          b.input('Style Name', 'Heading');
          await b.frame();
          const typingMs = performance.now() - typing;
          const before = await b.state(),
            began = performance.now();
          b.button('Preview style changes').click();
          await b.wait(() => b.button('Accept style changes'));
          await b.frame();
          const preparationMs = performance.now() - began;
          const count = document.querySelector(
            '[aria-label="Style change review"]',
          ).textContent;
          if (!/2500\s+cue references/.test(count))
            throw new Error(`Wrong reference count: ${count}`);
          const acceptedAt = performance.now();
          b.button('Accept style changes').click();
          await b.wait(
            async () =>
              (await b.state()).revision === before.revision + 1 && b.ready(),
          );
          await b.frame();
          const acceptMs = performance.now() - acceptedAt;
          const changed = await b.state();
          if (changed.cues.filter((c) => c.style === 'Heading').length !== 2500)
            throw new Error('Incomplete atomic rename');
          const undoAt = performance.now();
          b.button('Undo edit').click();
          await b.wait(
            async () =>
              (await b.state()).revision === before.revision + 2 && b.ready(),
          );
          await b.frame();
          const undoMs = performance.now() - undoAt;
          if ((await b.state()).dirty)
            throw new Error('Undo did not restore clean source');
          return { typingMs, preparationMs, acceptMs, undoMs };
        });
        for (const [key, value] of Object.entries(measured))
          result[key].push(value);
      }
      // Cancel and stale acceptance use actual owner commands, independent of
      // whether this small proposal finishes before the next UI polling tick.
      const guards = await browser.execute(async () => {
        const b = window.__styleBench,
          doc = await b.state();
        const request = {
          generation: doc.generation,
          revision: doc.revision,
          operation: {
            kind: 'edit',
            name: 'Title',
            fields: [{ name: 'Name', value: 'Heading' }],
          },
        };
        const id = await b.invoke('style_start', request);
        const began = performance.now();
        await b.invoke('style_cancel', { id });
        await b.frame();
        const cancelMs = performance.now() - began;
        await b.wait(
          async () =>
            (await b.invoke('style_view', { id })).phase === 'cancelled',
        );
        let rejected = false;
        try {
          await b.invoke('style_accept', { id });
        } catch {
          rejected = true;
        }
        if (!rejected) throw new Error('Cancelled style accepted');
        const newer = await b.invoke('style_start', request);
        await b.wait(
          async () =>
            (await b.invoke('style_view', { id: newer })).phase === 'reviewing',
        );
        const cue = doc.cues[0];
        const staleAt = performance.now();
        const changed = await b.invoke('document_action', {
          generation: doc.generation,
          revision: doc.revision,
          action: {
            kind: 'edit',
            cueId: cue.id,
            startMs: cue.startMs,
            endMs: cue.endMs,
            text: cue.text + '!',
          },
        });
        if ((await b.invoke('style_view', { id: newer })).phase !== 'stale')
          throw new Error('Edited context kept review current');
        await b.frame();
        const staleMs = performance.now() - staleAt;
        rejected = false;
        try {
          await b.invoke('style_accept', { id: newer });
        } catch {
          rejected = true;
        }
        if (!rejected) throw new Error('Stale style accepted');
        await b.invoke('document_action', {
          generation: changed.generation,
          revision: changed.revision,
          action: { kind: 'undo' },
        });
        if ((await b.state()).dirty)
          throw new Error('Guard cleanup did not restore source');
        return { cancelMs, staleMs };
      });
      Object.assign(result, guards);
      const interaction = [
        openMs,
        ...result.typingMs,
        ...result.acceptMs,
        ...result.undoMs,
        guards.cancelMs,
        guards.staleMs,
      ];
      result.maximumInteractionMs = Math.max(...interaction);
      result.passed = interaction.every(
        (n) => Number.isFinite(n) && n >= 0 && n < report.budgetMs,
      );
      await save();
      assert.ok(
        result.passed,
        `${extension} exceeded 1-second style interaction gate`,
      );
      await record({
        step: `actual ${extension} 10k style review/rename/Undo/cancel/stale`,
        maximumInteractionMs: result.maximumInteractionMs,
      });
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
