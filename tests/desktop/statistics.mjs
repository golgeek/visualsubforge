// Actual WebView/IPC and independent CLI parity. Fixtures have known source
// positions and cut times; only file choices and app-data paths are supplied.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { identity, makeVideos, time } from './timing-fixtures.mjs';
async function cliReport(cli, path, profile, selection) {
  const args = ['stats', '--subtitles', path, '--profile', profile, '--json'];
  if (selection.range)
    args.push(
      '--from-cue',
      String(selection.range.first),
      '--to-cue',
      String(selection.range.last),
    );
  if (selection.excluded.length)
    args.push('--exclude', selection.excluded.join(','));
  return new Promise((resolve, reject) =>
    execFile(
      cli,
      args,
      { timeout: 20000, maxBuffer: 32 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error && error.code !== 3) reject(error);
        else
          resolve({
            code: error?.code ?? 0,
            report: JSON.parse(stdout),
            stderr,
          });
      },
    ),
  );
}
async function prepare(root, context) {
  const source =
    '\uFEFF007\r\n00:00:00,000 --> 00:00:02,500\r\n{\\i1}A😀\r\nB\r\n\r\n10\r\n00:00:02,000 --> 00:00:02,400\r\n0\r\n\r\n15\r\n00:00:03,000 --> 00:00:04,500\r\n<i>Other text</i>\r\n\r\n20\r\n00:00:05,000 --> 00:00:07,000\r\n{\\i1}\r\n\r\n';
  await writeFile(join(root, 'statistics-small.srt'), source);
  const text =
    '{\\c&H00FFFF&}<i>This is a subtitle line with Unicode 日本語 😀.</i>\nAnother line to wrap.';
  await writeFile(
    join(root, 'statistics-large.srt'),
    Array.from(
      { length: 10000 },
      (_, i) =>
        `${i + 1}\n${time(i * 1400 + 650)} --> ${time(i * 1400 + 650 + (i % 2 ? 1300 : 400))}\n${text}\n`,
    ).join('\n'),
  );
  const videos = await makeVideos(root, 'statistics', [
    ['black', 3],
    ['red', 2],
    ['white', 3],
  ]);
  for (const name of ['small', 'large']) {
    const mediaPath = videos[name];
    const mediaIdentity = await identity(mediaPath);
    const profile = structuredClone(context.profile);
    profile.id = `statistics-desktop-${name}`;
    profile.name = `Statistics ${name}`;
    profile.revision = 1;
    profile.speedBasis = 'ordinary_v1';
    profile.timing = { cueGapMs: 80, sceneBeforeMs: 130, sceneAfterMs: 130 };
    await writeFile(
      join(root, `statistics-${name}.vsfproject`),
      JSON.stringify({
        version: 1,
        mediaPath,
        subtitlePath: join(root, `statistics-${name}.srt`),
        mediaIdentity,
        positionSeconds: 0,
        scenes: {
          version: 3,
          mode: 'visual',
          cuts:
            name === 'small'
              ? [3, 5]
              : Array.from({ length: 10000 }, (_, i) => (i + 1) * 1.4),
          identity: mediaIdentity,
        },
        options: {
          waveformStart: 0,
          waveformSpan: 20,
          followPlayback: false,
          followSubtitle: false,
          showScenes: true,
          timing: { ...context.timing, ...profile.timing },
          quality: profile,
        },
      }),
    );
  }
  return source;
}
export async function statisticsWorkflow({
  browser,
  menu,
  invoke,
  status,
  until,
  root,
  evidence,
  record,
  cli,
}) {
  const context = (await invoke('quality_settings')).current;
  const source = await prepare(root, context);
  const media = () =>
    invoke('media_action', { action: { kind: 'status' }, session: 0 });
  const ready = () =>
    until('statistics workspace ready', () =>
      browser.$('button=Statistics…').then((b) => b.isEnabled()),
    );
  async function report(selection = { range: null, excluded: [] }) {
    const doc = await status(),
      state = await media();
    const id = await invoke('statistics_start', {
      session: state.session,
      generation: doc.generation,
      revision: doc.revision,
      selection,
    });
    return until('statistics IPC report', async () => {
      const v = await invoke('statistics_view', { id, offset: 0, limit: 200 });
      assert.notEqual(v.phase, 'failed', v.error);
      return v.phase === 'ready' && v;
    });
  }
  async function open() {
    await ready();
    await (await browser.$('button=Statistics…')).click();
    const dialog = await browser.$(
      'dialog[aria-labelledby="statistics-title"]',
    );
    await until('statistics summary visible', () =>
      dialog.$('[aria-label="Metric summaries"]').then((e) => e.isExisting()),
    );
    return dialog;
  }
  async function close() {
    await (await browser.$('button=Close statistics')).click();
    await ready();
  }
  async function navigate(dialog, cue) {
    await (await dialog.$$(`button[title="Go to cue ${cue}"]`))[0].click();
    await ready();
  }
  await menu('openSubtitles');
  await until('statistics small input', async () => {
    const d = await status();
    return d.cues.length === 4 && d.cues[0].text.includes('A😀');
  });
  await ready();
  let before = await status();
  const profilePath = join(root, 'statistics-current.vsfpreset');
  await invoke('quality_profile_save', {
    path: profilePath,
    profile: context.profile,
  });
  const parity = [];
  for (const selection of [
    { range: null, excluded: [] },
    { range: { first: 1, last: 4 }, excluded: [2, 4] },
    { range: null, excluded: [1, 2, 3, 4] },
  ]) {
    const desktop = await report(selection);
    const command = await cliReport(
      cli,
      join(root, 'statistics-small.srt'),
      profilePath,
      selection,
    );
    assert.deepEqual(
      { summary: desktop.summary, findings: desktop.findings },
      command.report,
    );
    assert.equal(command.stderr, '');
    assert.deepEqual(await status(), before);
    parity.push({ selection, desktop, cli: command });
  }
  await writeFile(
    join(evidence, 'statistics-cli-parity.json'),
    JSON.stringify(parity, null, 2),
  );
  const text = await browser.$('[aria-label="Subtitle text"]');
  await text.setValue('Keep statistics draft');
  let dialog = await open();
  assert.match(await dialog.getText(), /not included in these\s+statistics/);
  assert.equal(
    (await dialog.$$('table[aria-label="Reading-speed distribution"] tbody tr'))
      .length,
    9,
  );
  assert.deepEqual(await status(), before);
  await browser.saveScreenshot(join(evidence, 'statistics-summary.png'));
  await close();
  assert.equal(await text.getValue(), 'Keep statistics draft');
  dialog = await open();
  await navigate(dialog, 2);
  await until('statistics draft guard', () =>
    browser.$('button=Cancel navigation').then((e) => e.isExisting()),
  );
  await (await browser.$('button=Cancel navigation')).click();
  assert.equal(await text.getValue(), 'Keep statistics draft');
  assert.deepEqual(await status(), before);
  dialog = await open();
  await navigate(dialog, 2);
  await (await browser.$('button=Discard draft and navigate')).click();
  await until(
    'statistics selected second cue',
    async () => (await text.getValue()) === '0',
  );
  assert.deepEqual(await status(), before);
  await text.setValue('Explicit statistics navigation');
  dialog = await open();
  await navigate(dialog, 1);
  await (await browser.$('button=Apply draft and navigate')).click();
  await until(
    'explicit statistics navigation Apply',
    async () => (await status()).revision === before.revision + 1,
  );
  await ready();
  assert.equal((await status()).cues[1].text, 'Explicit statistics navigation');
  await (await browser.$('button=Undo edit')).click();
  await ready();
  assert.deepEqual((await status()).cues, before.cues);
  before = await status();
  dialog = await open();
  await browser.execute(() => {
    const s = document.querySelector('[aria-label="Statistics scope"]');
    s.value = 'range';
    s.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await (await dialog.$('[aria-label="Last statistics cue"]')).setValue('4');
  await (
    await dialog.$('[aria-label="Excluded statistics cues"]')
  ).setValue('2, 4');
  await (await dialog.$('button=Calculate statistics')).click();
  await until('scoped statistics summary', async () =>
    /2 included cues/.test(await dialog.getText()),
  );
  assert.deepEqual(await status(), before);
  await browser.saveScreenshot(join(evidence, 'statistics-scoped.png'));
  await (
    await dialog.$('[aria-label="Excluded statistics cues"]')
  ).setValue('99');
  await (await dialog.$('button=Calculate statistics')).click();
  await until('invalid statistics scope', async () =>
    /within the selected scope/.test(await dialog.getText()),
  );
  assert.equal(
    await dialog
      .$('[aria-label="Metric summaries"]')
      .then((e) => e.isExisting()),
    false,
  );
  await close();
  assert.deepEqual(await status(), before);
  assert.equal(
    await readFile(join(root, 'statistics-small.srt'), 'utf8'),
    source,
  );
  const old = await report();
  const current = (await invoke('quality_settings')).current;
  const preferences = await invoke('preferences_get');
  await invoke('preferences_save', { preferences, epoch: current.epoch });
  const stale = await invoke('statistics_view', {
    id: old.id,
    offset: 0,
    limit: 100,
  });
  assert.equal(stale.phase, 'stale');
  assert.equal(stale.summary, null);
  assert.deepEqual(await status(), before);
  await record({
    step: 'real read-only statistics, full/range/all-excluded CLI parity, protected draft Cancel/Discard/Apply, exact source/history preservation, invalid scope and profile invalidation',
    cues: 4,
    parityScopes: parity.length,
  });
  const noMedia = await report();
  await menu('openProject');
  await ready();
  await until(
    'statistics small project loaded',
    async () => (await media()).loaded,
  );
  await assert.rejects(() =>
    invoke('statistics_view', { id: noMedia.id, offset: 0, limit: 100 }),
  );
  const scene = await report();
  assert.equal(scene.summary.sceneCoverage, 'checked');
  await menu('scanScenes');
  await assert.rejects(() =>
    invoke('statistics_view', { id: scene.id, offset: 0, limit: 100 }),
  );
  await until('completed statistics live rescan', async () => {
    try {
      return (await report()).summary.sceneCoverage === 'checked';
    } catch (e) {
      if (String(e).includes('finish cancelling')) return false;
      throw e;
    }
  });
  const small = await report();
  await menu('openProject');
  await until(
    'large statistics project',
    async () => (await status()).cues.length === 10000,
  );
  await ready();
  await assert.rejects(() =>
    invoke('statistics_view', { id: small.id, offset: 0, limit: 100 }),
  );
  await statisticsPerformance({
    browser,
    invoke,
    status,
    media,
    report,
    open,
    close,
    ready,
    until,
    evidence,
    record,
  });
  await menu('closeMedia');
  await ready();
  assert.equal((await report()).summary.sceneCoverage, 'noMedia');
}
async function statisticsPerformance({
  browser,
  invoke,
  status,
  media,
  report,
  open,
  close,
  ready,
  until,
  evidence,
  record,
}) {
  const baseline = await status(),
    state = await media();
  const result = {
    cues: 10000,
    cuts: 10000,
    budgetMs: 1000,
    status: 'running',
  };
  try {
    result.interaction = await browser.execute(
      async (doc, session) => {
        const invoke = window.__TAURI__.core.invoke;
        const id = await invoke('statistics_start', {
          session,
          ...doc,
          selection: { range: null, excluded: [] },
        });
        const input = document.querySelector('[aria-label="Subtitle text"]');
        const began = performance.now();
        input.value = 'Statistics performance draft';
        input.dispatchEvent(new Event('input', { bubbles: true }));
        await new Promise((r) => requestAnimationFrame(r));
        const typingMs = performance.now() - began;
        const start = performance.now();
        await invoke('statistics_cancel', { id });
        let v;
        do {
          v = await invoke('statistics_view', { id, offset: 0, limit: 100 });
          if (performance.now() - start > 1000)
            throw new Error('Statistics cancellation exceeded budget');
        } while (v.phase === 'cancelling');
        return {
          typingMs,
          cancellationMs: performance.now() - start,
          phase: v.phase,
        };
      },
      { generation: baseline.generation, revision: baseline.revision },
      state.session,
    );
    assert.equal(result.interaction.phase, 'cancelled');
    assert.ok(result.interaction.typingMs < 1000);
    await (await browser.$('button=Revert draft')).click();
    await ready();
    const began = Date.now();
    const dialog = await open();
    result.previewMs = Date.now() - began;
    assert.match(await dialog.getText(), /10000 included cues/);
    assert.equal((await dialog.$$('.finding')).length, 100);
    await (await dialog.$('button=Next findings')).click();
    await until('statistics page two', async () =>
      /101–200 of/.test(await dialog.getText()),
    );
    await browser.execute(() => {
      const d = document.querySelector('dialog[open]');
      d.scrollTop = d.scrollHeight;
    });
    await browser.saveScreenshot(
      join(evidence, 'statistics-10000-outliers.png'),
    );
    assert.deepEqual(await status(), baseline);
    await close();
    const r = await report();
    assert.equal(r.summary.includedCues, 10000);
    assert.equal(r.summary.rs.count, 5000);
    assert.equal(r.summary.unavailableRs.total, 5000);
    assert.equal(r.summary.gapsMs.count, 9999);
    assert.equal(r.summary.overlappingPairs, 0);
    assert.ok(r.findings.length <= 200);
    await assert.rejects(() =>
      invoke('statistics_view', { id: r.id, offset: 0, limit: 201 }),
    );
    const scoped = await report({
      range: { first: 1001, last: 9000 },
      excluded: Array.from({ length: 2000 }, (_, i) => 1004 + i * 4),
    });
    assert.equal(scoped.summary.includedCues, 6000);
    assert.equal(scoped.summary.rs.count, 2000);
    assert.equal(scoped.summary.gapsMs.count, 4000);
    assert.deepEqual(await status(), baseline);
    const old = await report();
    await browser
      .$('[aria-label="Subtitle text"]')
      .then((e) => e.setValue('Invalidate statistics'));
    const invalidateStart = Date.now();
    await (await browser.$('button=Apply edit')).click();
    await until('statistics edit invalidation', async () => {
      const v = await invoke('statistics_view', {
        id: old.id,
        offset: 0,
        limit: 100,
      });
      return (
        v.phase === 'stale' && v.summary === null && v.findings.length === 0
      );
    });
    result.invalidationMs = Date.now() - invalidateStart;
    assert.ok(result.invalidationMs < 1000);
    await ready();
    await (await browser.$('button=Undo edit')).click();
    await ready();
    assert.deepEqual((await status()).cues, baseline.cues);
    const nav = await open();
    const navigationStart = Date.now();
    await (await nav.$$('button[title="Go to cue 2"]'))[0].click();
    await ready();
    result.navigationMs = Date.now() - navigationStart;
    assert.ok(result.navigationMs < 1000);
    assert.equal(
      await browser.$('[aria-label="Cue start"]').then((e) => e.getValue()),
      time(baseline.cues[1].startMs),
    );
    result.status = 'passed';
    await record({
      step: 'real 10000-cue statistics, scoped exclusions, bounded pages, read-only history, cancellation, typing, navigation and invalidation',
      ...result,
    });
  } catch (error) {
    result.status = 'failed';
    result.error = String(error.stack ?? error);
    throw error;
  } finally {
    await writeFile(
      join(evidence, 'statistics-performance.json'),
      JSON.stringify(result, null, 2),
    );
  }
}
