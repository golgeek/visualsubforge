import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';

// Reuse the loaded 10,000-cue quality fixture after its checks. Timing uses the
// actual webview clock/IPC and generated DOM events, not physical input latency.
export async function textPerformance({
  browser,
  status,
  until,
  evidence,
  record,
}) {
  const baseline = await status();
  assert.equal(baseline.cues.length, 10_000);
  const result = { budgetMs: 1000, cues: 10_000, status: 'running' };
  const ready = () =>
    until('text performance workspace ready', () =>
      browser.$('button=Find / Replace…').then((b) => b.isEnabled()),
    );
  try {
    await ready();
    result.cancelAndInteraction = await browser.execute(
      async (doc) => {
        const invoke = window.__TAURI__.core.invoke;
        const start = performance.now();
        const id = await invoke('text_start', {
          generation: doc.generation,
          revision: doc.revision,
          operation: {
            kind: 'replace',
            query: 'line',
            replacement: 'sentence',
            caseSensitive: false,
          },
        });
        const input = document.querySelector('[aria-label="Subtitle text"]');
        const typingStart = performance.now();
        input.value = 'Performance text draft';
        input.dispatchEvent(new Event('input', { bubbles: true }));
        await new Promise((resolve) => requestAnimationFrame(resolve));
        const typingMs = performance.now() - typingStart;
        const cancelStart = performance.now();
        await invoke('text_cancel', { id });
        let view;
        do {
          view = await invoke('text_view', { id, offset: 0, limit: 100 });
          if (performance.now() - cancelStart > 1000)
            throw new Error('Text cancellation exceeded one second');
        } while (view.phase === 'cancelling');
        return {
          admissionMs: cancelStart - start,
          typingMs,
          cancellationMs: performance.now() - cancelStart,
          phase: view.phase,
        };
      },
      { generation: baseline.generation, revision: baseline.revision },
    );
    assert.equal(result.cancelAndInteraction.phase, 'cancelled');
    assert.ok(result.cancelAndInteraction.typingMs < result.budgetMs);
    await (await browser.$('button=Revert draft')).click();
    await (await browser.$('button=Find / Replace…')).click();
    let dialog = await browser.$('dialog[aria-labelledby="text-title"]');
    await until('10k text dialog', () => dialog.isDisplayed());
    await browser.execute(() => {
      const select = document.querySelector(
        'dialog[aria-labelledby="text-title"] select',
      );
      select.value = 'replace';
      select.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await until('replacement field', () =>
      dialog.$('[aria-label="Replace with"]').then((e) => e.isExisting()),
    );
    await (await dialog.$('[aria-label="Find text"]')).setValue('line');
    await (await dialog.$('[aria-label="Replace with"]')).setValue('sentence');
    const started = Date.now();
    await (await dialog.$('button=Preview text changes')).click();
    await until('10k text preview complete', async () =>
      (await dialog.$('button=Accept Replace All')).isEnabled(),
    );
    result.previewMs = Date.now() - started;
    assert.ok((await dialog.getText()).includes('10000 changed cues'));
    result.visibleRows = await browser.execute(
      () =>
        document.querySelectorAll('[aria-label="Text proposal"] .row').length,
    );
    assert.equal(result.visibleRows, 100);
    await browser.saveScreenshot(join(evidence, 'text-10000-preview.png'));
    await (await dialog.$('button=Next cues')).click();
    await until('next text result page', async () =>
      (await dialog.getText()).includes('Cue 101'),
    );
    const acceptStart = Date.now();
    await (await dialog.$('button=Accept Replace All')).click();
    await until(
      '10k text batch accepted',
      async () => !(await dialog.isExisting()),
    );
    await ready();
    result.acceptanceMs = Date.now() - acceptStart;
    const accepted = await status();
    assert.equal(accepted.revision, baseline.revision + 1);
    assert.equal(accepted.requiresSaveAs, baseline.requiresSaveAs);
    for (let i = 0; i < 10_000; i++)
      assert.deepEqual(accepted.cues[i], {
        ...baseline.cues[i],
        text: baseline.cues[i].text.replaceAll('line', 'sentence'),
      });
    assert.ok(
      result.acceptanceMs < result.budgetMs,
      `Batch acceptance ${result.acceptanceMs} ms`,
    );
    await (await browser.$('button=Undo edit')).click();
    await until(
      '10k batch Undo',
      async () => (await status()).revision === baseline.revision + 2,
    );
    await ready();
    const restored = await status();
    assert.deepEqual(restored.cues, baseline.cues);
    assert.equal(restored.dirty, baseline.dirty);
    // Keep this proposal outside the modal so close/dispose does not cancel it
    // before the accepted UI edit exercises backend revision invalidation.
    const prepared = await browser.execute(
      async (doc) => {
        const invoke = window.__TAURI__.core.invoke;
        const id = await invoke('text_start', {
          ...doc,
          operation: { kind: 'find', query: 'line', caseSensitive: false },
        });
        const deadline = performance.now() + 10_000;
        let view;
        do {
          view = await invoke('text_view', { id, offset: 0, limit: 200 });
          if (performance.now() > deadline)
            throw new Error('Find preview timed out');
          if (view.phase !== 'preparing' && view.phase !== 'reviewing')
            throw new Error(`Find failed: ${view.phase}`);
          await new Promise((resolve) => setTimeout(resolve, 10));
        } while (view.phase !== 'reviewing');
        const times = [];
        for (let i = 0; i < 20; i++) {
          const start = performance.now();
          const page = await invoke('text_view', {
            id,
            offset: i * 200,
            limit: 200,
          });
          if (page.rows.length !== 200)
            throw new Error('Incorrect text page size');
          times.push(performance.now() - start);
        }
        return { id, times };
      },
      { generation: restored.generation, revision: restored.revision },
    );
    result.pageP95Ms = [...prepared.times].sort((a, b) => a - b)[18];
    assert.ok(result.pageP95Ms < result.budgetMs);
    const editor = await browser.$('[aria-label="Subtitle text"]');
    const oldText = await editor.getValue();
    await editor.setValue(`${oldText}!`);
    const invalidationStart = Date.now();
    await (await browser.$('button=Apply edit')).click();
    await until('accepted edit invalidates text preview', async () =>
      browser.execute(async (id) => {
        const view = await window.__TAURI__.core.invoke('text_view', {
          id,
          offset: 0,
          limit: 100,
        });
        return view.phase === 'stale' && view.rows.length === 0;
      }, prepared.id),
    );
    result.invalidationMs = Date.now() - invalidationStart;
    assert.ok(result.invalidationMs < result.budgetMs);
    await ready();
    await (await browser.$('button=Undo edit')).click();
    await until(
      '10k text fixture restored',
      async () => (await status()).revision === restored.revision + 2,
    );
    await ready();
    assert.deepEqual((await status()).cues, baseline.cues);
    result.status = 'passed';
    await record({
      step: '10,000-cue real text proposal, cancellation, interaction, paging, atomic acceptance/Undo and invalidation',
      ...result,
    });
  } catch (e) {
    result.status = 'failed';
    result.error = String(e.stack ?? e);
    throw e;
  } finally {
    await writeFile(
      join(evidence, 'text-performance.json'),
      JSON.stringify(result, null, 2),
    );
  }
}
