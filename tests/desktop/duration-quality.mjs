// Real media-enabled WebView/IPC workflow. Synthetic media has known cut times;
// prepared project caches exercise restoration separately from the live rescan.
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { time, identity, makeVideos } from './timing-fixtures.mjs';
async function prepare(root, context) {
  const { small, large } = await makeVideos(root, 'duration', [
    ['black', 3],
    ['red', 4],
    ['white', 9],
  ]);
  const source =
    '\uFEFF1\r\n00:00:01,000 --> 00:00:01,400\r\n{\\i1}0123456789é😀abcdefg\r\n\r\n2\r\n00:00:05,000 --> 00:00:13,000\r\n0123456789\r\n\r\n3\r\n00:00:14,000 --> 00:00:14,400\r\n{\\k20}Timed\r\n\r\n';
  await writeFile(join(root, 'duration-small.srt'), source);
  const text =
    '{\\c&H00FFFF&}<i>This is a subtitle line with Unicode 日本語 😀.</i>\nAnother line to wrap.';
  const big = Array.from(
    { length: 10000 },
    (_, i) =>
      `${i + 1}\n${time(i * 1400 + 650)} --> ${time(i * 1400 + 650 + (i % 2 ? 1300 : 400))}\n${text}\n`,
  ).join('\n');
  await writeFile(join(root, 'duration-large.srt'), big);
  for (const [name, path, cuts] of [
    ['small', small, [3, 7]],
    ['large', large, Array.from({ length: 10000 }, (_, i) => (i + 1) * 1.4)],
  ]) {
    const profile = structuredClone(context.profile);
    profile.id = `duration-desktop-${name}`;
    profile.name = `Desktop duration ${name}`;
    profile.revision = 1;
    profile.timing = { cueGapMs: 80, sceneBeforeMs: 130, sceneAfterMs: 130 };
    profile.rules.duration = {
      enabled: true,
      minimumMs: 800,
      maximumMs: name === 'large' ? 1200 : 3000,
    };
    profile.rules.speed = {
      enabled: true,
      minimumRs: 5,
      maximumRsExclusive: name === 'large' ? 500 : 35,
      minimumCps: name === 'large' ? 60 : 5,
      maximumCps: name === 'large' ? 100 : 10,
    };
    const mediaIdentity = await identity(path);
    await writeFile(
      join(root, `duration-${name}.vsfproject`),
      JSON.stringify({
        version: 1,
        mediaPath: path,
        subtitlePath: join(root, `duration-${name}.srt`),
        mediaIdentity,
        positionSeconds: 0,
        scenes: { version: 3, mode: 'visual', cuts, identity: mediaIdentity },
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

export async function durationQuality({
  browser,
  menu,
  invoke,
  status,
  until,
  root,
  evidence,
  record,
}) {
  const context = (await invoke('quality_settings')).current;
  const source = await prepare(root, context);
  const ready = () =>
    until('duration workspace ready', () =>
      browser.$('button=Repair duration / speed…').then((b) => b.isEnabled()),
    );
  const media = () =>
    invoke('media_action', { action: { kind: 'status' }, session: 0 });
  const quality = async (rule = null) => {
    const doc = await status(),
      state = await media();
    return invoke('quality_read', {
      request: {
        session: state.session,
        generation: doc.generation,
        revision: doc.revision,
        run: true,
        restart: false,
        offset: 0,
        limit: 200,
        metricsOffset: 0,
        metricsLimit: 0,
        selected: 1,
        rule,
        severity: null,
        draft: null,
      },
    });
  };
  const proposal = async () => {
    const doc = await status(),
      state = await media();
    const id = await invoke('duration_repair_start', {
      selectedCueId: null,
      session: state.session,
      generation: doc.generation,
      revision: doc.revision,
    });
    const view = await until('completed duration IPC proposal', async () => {
      const v = await invoke('duration_repair_view', {
        id,
        offset: 0,
        limit: 100,
      });
      assert.notEqual(v.phase, 'failed', v.error);
      return v.phase === 'reviewing' && v;
    });
    return view;
  };
  await menu('openProject');
  await until('small duration project', async () => {
    const d = await status();
    return d.cues.length === 3 && d.cues[0].text.includes('0123456789é');
  });
  await ready();
  let checked = await until('duration findings', async () => {
    const v = await quality();
    return v.phase === 'ready' && v.sceneCoverage === 'checked' && v;
  });
  assert.equal(checked.findingsCount, 9);
  let before = await status();
  const text = await browser.$('[aria-label="Subtitle text"]');
  await text.setValue('Preserve duration draft');
  await (await browser.$('button=Repair duration / speed…')).click();
  await until('duration draft guard', () =>
    browser.$('button=Cancel duration review').then((b) => b.isExisting()),
  );
  await (await browser.$('button=Cancel duration review')).click();
  await ready();
  assert.equal(await text.getValue(), 'Preserve duration draft');
  assert.deepEqual(await status(), before);
  await (await browser.$('button=Revert draft')).click();
  await (await browser.$('button=Repair duration / speed…')).click();
  let selectedDialog = await browser.$(
    'dialog[aria-labelledby="duration-repair-title"]',
  );
  await until('selected duration dialog', () => selectedDialog.isDisplayed());
  await (await selectedDialog.$('button=Create duration preview')).click();
  await until('selected duration proposal', async () =>
    (await selectedDialog.getText()).includes('1 safe changes'),
  );
  assert.equal((await selectedDialog.$$('tbody tr')).length, 1);
  assert.deepEqual(await status(), before);
  await (await selectedDialog.$('button=Accept duration changes')).click();
  await ready();
  assert.deepEqual(
    (await status()).cues.map((c) => c.endMs),
    [3130, 13000, 14400],
  );
  await (await browser.$('button=Undo edit')).click();
  await ready();
  assert.deepEqual((await status()).cues, before.cues);
  before = await status();
  await (await browser.$('button=Repair duration / speed…')).click();
  let dialog = await browser.$(
    'dialog[aria-labelledby="duration-repair-title"]',
  );
  await until('duration dialog', () => dialog.isDisplayed());
  await browser.execute(() => {
    const scope = document.querySelector('[aria-label="Repair scope"]');
    scope.value = 'document';
    scope.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await (await dialog.$('button=Create duration preview')).click();
  await until('duration preview rows', async () =>
    (await dialog.getText()).includes('2 safe changes'),
  );
  assert.match(await dialog.getText(), /timing review/i);
  assert.deepEqual(await status(), before);
  await browser.saveScreenshot(join(evidence, 'duration-preview.png'));
  await (await dialog.$('button=Accept duration changes')).click();
  await ready();
  const accepted = await status();
  assert.equal(accepted.revision, before.revision + 1);
  assert.deepEqual(
    accepted.cues.map((c) => [c.startMs, c.endMs]),
    [
      [1000, 3130],
      [5000, 6870],
      [14000, 14400],
    ],
  );
  assert.deepEqual(
    accepted.cues.map((c) => [c.id, c.text]),
    before.cues.map((c) => [c.id, c.text]),
  );
  await assert.rejects(() =>
    invoke('document_action', {
      action: { kind: 'save', path: root },
      generation: accepted.generation,
      revision: accepted.revision,
    }),
  );
  assert.deepEqual(await status(), accepted);
  await (await browser.$('button=Undo edit')).click();
  await ready();
  assert.deepEqual((await status()).cues, before.cues);
  await (await browser.$('button=Redo edit')).click();
  await ready();
  assert.deepEqual((await status()).cues, accepted.cues);
  await menu('saveAs');
  await until('saved duration copy', async () => !(await status()).dirty);
  const expected = source
    .replace('00:00:01,400', '00:00:03,130')
    .replace('00:00:13,000', '00:00:06,870');
  assert.equal(
    await readFile(join(root, 'duration-copy.srt'), 'utf8'),
    expected,
  );
  assert.equal(
    await readFile(join(root, 'duration-small.srt'), 'utf8'),
    source,
  );
  await menu('openSubtitles');
  await ready();
  assert.deepEqual((await status()).cues, accepted.cues);
  const old = await proposal();
  await menu('scanScenes');
  await until('scene scan invalidates completed preview', async () => {
    try {
      await invoke('duration_repair_view', {
        id: old.id,
        offset: 0,
        limit: 100,
      });
      return false;
    } catch {
      return true;
    }
  });
  await assert.rejects(() => invoke('duration_repair_accept', { id: old.id }));
  checked = await until(
    'actual completed FFmpeg scene scan',
    async () => {
      const v = await quality();
      return v.phase === 'ready' && v.sceneCoverage === 'checked' && v;
    },
    60000,
  );
  await record({
    step: 'real selected/all duration previews, protected draft, fixed starts/tags, failed save, atomic Undo/Redo, exact save/reopen and rescan rejection',
    beforeFindings: 9,
    changed: 2,
    skipped: 1,
    afterLiveScanFindings: checked.findingsCount,
  });
  const mediaOld = await proposal();
  await menu('openProject');
  await until(
    '10k scene project',
    async () => (await status()).cues.length === 10000,
    60000,
  );
  await ready();
  await assert.rejects(() =>
    invoke('duration_repair_accept', { id: mediaOld.id }),
  );
  await durationPerformance({
    browser,
    invoke,
    status,
    media,
    proposal,
    quality,
    ready,
    until,
    evidence,
    record,
  });
  await menu('closeMedia');
  await ready();
  const absent = await until('media-free scene report', async () => {
    const v = await quality();
    return v.phase === 'ready' && v.sceneCoverage === 'noMedia' && v;
  });
  assert.equal(
    absent.findings.filter((f) => f.rule.startsWith('scene_')).length,
    0,
  );
}

async function durationPerformance({
  browser,
  invoke,
  status,
  media,
  proposal,
  quality,
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
    result.checking = await until('10k duration findings', async () => {
      const v = await quality('duration_short');
      return v.phase === 'ready' && v;
    });
    assert.equal(result.checking.filteredCount, 5000);
    assert.equal((await quality('duration_long')).filteredCount, 5000);
    delete result.checking;
    result.interaction = await browser.execute(
      async (doc, session) => {
        const invoke = window.__TAURI__.core.invoke;
        const id = await invoke('duration_repair_start', {
          selectedCueId: null,
          session,
          generation: doc.generation,
          revision: doc.revision,
        });
        const input = document.querySelector('[aria-label="Subtitle text"]');
        const start = performance.now();
        input.value = 'Duration performance draft';
        input.dispatchEvent(new Event('input', { bubbles: true }));
        await new Promise((r) => requestAnimationFrame(r));
        const typingMs = performance.now() - start,
          cancelStart = performance.now();
        await invoke('duration_repair_cancel', { id });
        let view;
        do {
          view = await invoke('duration_repair_view', {
            id,
            offset: 0,
            limit: 100,
          });
          if (performance.now() - cancelStart > 1000)
            throw new Error('Duration cancellation exceeded budget');
        } while (view.phase === 'cancelling');
        return {
          typingMs,
          cancellationMs: performance.now() - cancelStart,
          phase: view.phase,
        };
      },
      { generation: baseline.generation, revision: baseline.revision },
      state.session,
    );
    assert.equal(result.interaction.phase, 'cancelled');
    assert.ok(result.interaction.typingMs < 1000);
    await (await browser.$('button=Revert draft')).click();
    await ready();
    await (await browser.$('button=Repair duration / speed…')).click();
    const dialog = await browser.$(
      'dialog[aria-labelledby="duration-repair-title"]',
    );
    await until('10k duration dialog', () => dialog.isDisplayed());
    await browser.execute(() => {
      const scope = document.querySelector('[aria-label="Repair scope"]');
      scope.value = 'document';
      scope.dispatchEvent(new Event('change', { bubbles: true }));
    });
    const began = Date.now();
    await (await dialog.$('button=Create duration preview')).click();
    await until(
      '10k completed scene preview',
      async () => (await dialog.getText()).includes('10000 safe changes'),
      20000,
    );
    result.previewMs = Date.now() - began;
    assert.equal((await dialog.$$('tbody tr')).length, 100);
    await (await dialog.$('button=Next candidates')).click();
    await until('10k second scene page', async () =>
      (await dialog.getText()).includes('101–200 of 10000'),
    );
    await browser.saveScreenshot(join(evidence, 'duration-10000-preview.png'));
    const acceptStart = Date.now();
    await (await dialog.$('button=Accept duration changes')).click();
    await ready();
    result.acceptanceMs = Date.now() - acceptStart;
    assert.ok(
      result.acceptanceMs < result.budgetMs,
      'Duration acceptance exceeded budget',
    );
    const changed = await status();
    assert.equal(changed.revision, baseline.revision + 1);
    assert.equal(
      changed.cues.filter((c, i) => c.endMs !== baseline.cues[i].endMs).length,
      10000,
    );
    assert.deepEqual(
      changed.cues.map((c) => [c.id, c.text, c.startMs]),
      baseline.cues.map((c) => [c.id, c.text, c.startMs]),
    );
    assert.ok(
      changed.cues.every(
        (c, i) => c.endMs === c.startMs + (i % 2 ? 1116 : 880),
      ),
    );
    await (await browser.$('button=Undo edit')).click();
    await ready();
    assert.deepEqual((await status()).cues, baseline.cues);
    const captured = await proposal();
    const before = await status();
    await browser
      .$('[aria-label="Subtitle text"]')
      .then((e) => e.setValue('Invalidate duration proposal'));
    const invalidationStart = Date.now();
    await (await browser.$('button=Apply edit')).click();
    await until('10k scene invalidation', async () => {
      const v = await invoke('duration_repair_view', {
        id: captured.id,
        offset: 0,
        limit: 100,
      });
      return v.phase === 'stale' && v.rows.length === 0;
    });
    result.invalidationMs = Date.now() - invalidationStart;
    assert.ok(result.invalidationMs < 1000);
    await ready();
    await (await browser.$('button=Undo edit')).click();
    await ready();
    assert.equal((await status()).revision, before.revision + 2);
    assert.deepEqual((await status()).cues, baseline.cues);
    result.status = 'passed';
    await record({
      step: 'real 10000-cue duration check, mixed extensions/trims, paging, full acceptance/Undo, cancellation, interaction and invalidation',
      ...result,
    });
  } catch (error) {
    result.status = 'failed';
    result.error = String(error.stack ?? error);
    throw error;
  } finally {
    await writeFile(
      join(evidence, 'duration-performance.json'),
      JSON.stringify(result, null, 2),
    );
  }
}
