// Runs inside the existing identified 10k ASS/SSA desktop fixture lane.
import assert from 'node:assert/strict';
import { join } from 'node:path';
export async function batchPerformance({
  browser,
  status,
  evidence,
  extension,
}) {
  const baseline = await status();
  const result = await browser.execute(async () => {
    const b = window.__assBench;
    const times = {};
    let started = performance.now();
    document.querySelector('.cue-row').dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'a',
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
    await b.wait(
      () =>
        document
          .querySelector('.table-head')
          .textContent.includes('10000 selected') && b.ready(),
    );
    await b.frame();
    times.selectAllMs = performance.now() - started;
    b.button('Shift / align cues…').click();
    await b.wait(() => document.querySelector('dialog[open]'));
    await b.frame();
    started = performance.now();
    b.button('Create batch preview').click();
    await b.wait(() => !b.button('Accept batch timings').disabled);
    await b.frame();
    times.previewMs = performance.now() - started;
    const rows = document.querySelectorAll('dialog tbody tr').length;
    b.button('Next candidates').click();
    await b.wait(
      () =>
        document.querySelector('dialog tbody td')?.textContent.trim() === '101',
    );
    const before = await b.state();
    started = performance.now();
    b.button('Accept batch timings').click();
    await b.wait(
      async () =>
        !document.querySelector('dialog[open]') &&
        (await b.state()).revision === before.revision + 1 &&
        b.ready(),
    );
    await b.frame();
    times.acceptMs = performance.now() - started;
    const shifted = await b.state();
    if (
      !shifted.cues.every(
        (c, i) =>
          c.startMs === before.cues[i].startMs + 100 &&
          c.endMs === before.cues[i].endMs + 100 &&
          c.text === before.cues[i].text &&
          c.style === before.cues[i].style,
      )
    )
      throw new Error('Batch corrupted timings or source fields');
    if (
      !document
        .querySelector('.table-head')
        .textContent.includes('10000 selected')
    )
      throw new Error('Acceptance lost selection');
    started = performance.now();
    b.button('Undo edit').click();
    await b.wait(
      async () =>
        (await b.state()).revision === before.revision + 2 && b.ready(),
    );
    await b.frame();
    times.undoMs = performance.now() - started;
    b.button('Redo edit').click();
    await b.wait(
      async () =>
        (await b.state()).revision === before.revision + 3 && b.ready(),
    );
    b.button('Undo edit').click();
    await b.wait(
      async () =>
        (await b.state()).revision === before.revision + 4 && b.ready(),
    );
    return { times, rows };
  });
  assert.equal(result.rows, 100);
  assert.deepEqual((await status()).cues, baseline.cues);
  await browser.saveScreenshot(
    join(evidence, `${extension}-batch-selection.png`),
  );
  const asyncResult = await browser.execute(async () => {
    const b = window.__assBench;
    const doc = await b.state();
    const media = await b.invoke('media_action', {
      action: { kind: 'status' },
      session: 0,
    });
    const start = () =>
      b.invoke('batch_timing_start', {
        session: media.session,
        generation: doc.generation,
        revision: doc.revision,
        request: {
          scope: { kind: 'all' },
          change: { kind: 'shift', earlier: false, amountMs: 100 },
        },
      });
    const view = (id) =>
      b.invoke('batch_timing_view', { id, offset: 0, limit: 200 });
    const id = await start();
    const editor = document.querySelector('[aria-label="Subtitle text"]');
    const original = editor.value;
    let began = performance.now();
    b.input('Subtitle text', `${original}!`);
    await b.frame();
    const typingMs = performance.now() - began;
    began = performance.now();
    await b.invoke('batch_timing_cancel', { id });
    await b.wait(async () => (await view(id)).phase === 'cancelled');
    const cancellationMs = performance.now() - began;
    b.button('Revert draft').click();
    await b.frame();
    const next = await start();
    const ready = await b.wait(async () => {
      const v = await view(next);
      if (v.error) throw new Error(v.error);
      return v.phase === 'reviewing' && v;
    });
    b.input('Subtitle text', `${original}!`);
    await b.frame();
    began = performance.now();
    b.button('Apply edit').click();
    await b.wait(async () => (await view(next)).phase === 'stale' && b.ready());
    const invalidationMs = performance.now() - began;
    let rejected = false;
    try {
      await b.invoke('batch_timing_accept', { id: next });
    } catch {
      rejected = true;
    }
    if (!rejected) throw new Error('Stale batch accepted');
    b.button('Undo edit').click();
    await b.wait(
      async () => (await b.state()).revision === doc.revision + 2 && b.ready(),
    );
    return {
      typingMs,
      cancellationMs,
      invalidationMs,
      pageRows: ready.rows.length,
    };
  });
  assert.equal(asyncResult.pageRows, 200);
  Object.assign(result.times, {
    typingMs: asyncResult.typingMs,
    cancellationMs: asyncResult.cancellationMs,
    invalidationMs: asyncResult.invalidationMs,
  });
  for (const [name, value] of Object.entries(result.times))
    assert.ok(
      value < 1000,
      `${extension} batch ${name}: ${value} ms exceeds 1000 ms`,
    );
  assert.deepEqual((await status()).cues, baseline.cues);
  return result;
}
