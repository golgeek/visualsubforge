import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { makeVideos, time } from './timing-fixtures.mjs';

export async function batchTiming({
  restart,
  root,
  evidence,
  menu,
  invoke,
  status,
  until,
  record,
}) {
  const { small } = await makeVideos(root, 'batch', [
    ['black', 7],
    ['white', 7],
  ]);
  const path = join(root, 'batch.srt'),
    output = join(root, 'batch-copy.srt');
  const original =
    '\uFEFF' +
    Array.from(
      { length: 6 },
      (_, i) =>
        `${i + 1}\r\n${time(1001 + i * 2000)} --> ${time(1803 + i * 2000)}\r\n{\\k20\\move(1,2,3,4,0,500)}Cue ${i + 1} 世界\r\n\r\n`,
    ).join('');
  await writeFile(path, original);
  const browser = await restart([
    { kind: 'media', path: small },
    { kind: 'subtitles', path },
    { kind: 'saveSubtitles', path: join(root, 'absent', 'batch.srt') },
    { kind: 'saveSubtitles', path: output },
  ]);
  await menu('openMedia');
  const media = await until('batch media ready', async () => {
    const m = await invoke('media_action', {
      action: { kind: 'status' },
      session: 0,
    });
    return m.loaded && !m.seeking && m;
  });
  await menu('openSubtitles');
  await until(
    'batch subtitles',
    async () => (await status()).cues.length === 6,
  );
  const baseline = await status();
  await browser.execute(async () => {
    const wait = async (probe) => {
      const deadline = performance.now() + 4000;
      while (performance.now() < deadline) {
        if (await probe()) return;
        await new Promise((r) => setTimeout(r, 10));
      }
      throw new Error('Batch UI did not settle');
    };
    const button = (text) =>
      [...document.querySelectorAll('button')].find(
        (b) => b.textContent.trim() === text,
      );
    const click = async (text) => {
      await wait(() => button(text) && !button(text).disabled);
      button(text).click();
    };
    const input = (label, value) => {
      const f = document.querySelector(`[aria-label="${label}"]`);
      f.value = value;
      f.dispatchEvent(
        new Event(f.tagName === 'SELECT' ? 'change' : 'input', {
          bubbles: true,
        }),
      );
    };
    window.__batchUI = { wait, button, click, input };
    const row = document.querySelector('[data-cue-id="3"]');
    row.dispatchEvent(
      new MouseEvent('click', {
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
    await wait(
      () =>
        document.querySelectorAll('.cue-row[aria-pressed="true"]').length ===
          2 && row.getAttribute('aria-disabled') === 'false',
    );
  });
  await invoke('media_action', {
    action: { kind: 'seek', seconds: 5.015 },
    session: media.session,
  });
  const settled = await until('captured playhead', async () => {
    const m = await invoke('media_action', {
      action: { kind: 'status' },
      session: media.session,
    });
    return !m.seeking && Math.abs(m.positionSeconds - 5.015) < 0.05 && m;
  });
  const targetMs = Math.round(settled.positionSeconds * 1000);
  const offsetMs = targetMs - 1001;
  // Let the frontend's normal media poll observe the settled native position.
  await until('projected playhead', async () =>
    browser.execute(
      (target) =>
        document.querySelector('output')?.textContent.includes(target),
      time(targetMs),
    ),
  );
  await browser.execute(async (target) => {
    const b = window.__batchUI;
    await b.click('Shift / align cues…');
    await b.wait(() =>
      document.querySelector('[aria-label="Batch operation"]'),
    );
    b.input('Batch operation', 'align');
    await b.click('Create batch preview');
    await b.wait(() => !b.button('Accept batch timings').disabled);
    const table = document.querySelector('dialog[open]');
    if (
      !table.textContent.includes(target) ||
      table.querySelectorAll('tbody tr').length !== 2
    )
      throw new Error('Incorrect native playhead/selection review');
  }, time(targetMs));
  await browser.saveScreenshot(join(evidence, 'batch-native-review.png'));
  await browser.execute(() => window.__batchUI.click('Accept batch timings'));
  const accepted = await until('batch alignment accepted', async () => {
    const d = await status();
    return d.revision === baseline.revision + 1 && d;
  });
  for (let i = 0; i < 6; i++) {
    const delta = [0, 2].includes(i) ? offsetMs : 0;
    assert.equal(accepted.cues[i].startMs, baseline.cues[i].startMs + delta);
    assert.equal(accepted.cues[i].endMs, baseline.cues[i].endMs + delta);
    assert.equal(accepted.cues[i].text, baseline.cues[i].text);
  }
  await until('accepted native preview revision', async () => {
    const m = await invoke('media_action', {
      action: { kind: 'status' },
      session: media.session,
    });
    assert.equal(m.subtitlePreviewError, null);
    return (
      m.previewDocumentRevision === accepted.revision &&
      m.previewDocumentGeneration === accepted.generation
    );
  });
  await browser.execute(() => window.__batchUI.click('Undo edit'));
  await until(
    'batch undo',
    async () => (await status()).revision === accepted.revision + 1,
  );
  assert.deepEqual((await status()).cues, baseline.cues);
  await browser.execute(() => window.__batchUI.click('Redo edit'));
  await until(
    'batch redo',
    async () => (await status()).revision === accepted.revision + 2,
  );
  assert.deepEqual((await status()).cues, accepted.cues);
  const beforeFailure = await status();
  await menu('saveAs');
  await until('failed batch save', async () =>
    browser.execute(
      () =>
        document.querySelector('main').getAttribute('aria-busy') === 'false',
    ),
  );
  assert.deepEqual(await status(), beforeFailure);
  await menu('saveAs');
  await until(
    'saved batch output',
    async () => (await status()).path === output && !(await status()).dirty,
  );
  const expected = original
    .replace(
      '00:00:01,001 --> 00:00:01,803',
      `${time(1001 + offsetMs)} --> ${time(1803 + offsetMs)}`,
    )
    .replace(
      '00:00:05,001 --> 00:00:05,803',
      `${time(5001 + offsetMs)} --> ${time(5803 + offsetMs)}`,
    );
  assert.equal(await readFile(output, 'utf8'), expected);
  assert.equal(await readFile(path, 'utf8'), original);
  await record({
    step: 'Batch native paused-playhead alignment, disjoint selection, one-step Undo/Redo, failed save and exact tagged-SRT output',
    input: path,
    output,
    offsetMs,
    requestedSeekSeconds: 5.015,
    capturedPositionSeconds: settled.positionSeconds,
    sourcePreserved: true,
    previewRevision: accepted.revision,
    compositionObserved: false,
  });
}
