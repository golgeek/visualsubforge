// Real media-enabled WebView/IPC workflow. Synthetic media has known cut times;
// prepared project caches exercise restoration separately from the live rescan.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { readFile, writeFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
const run = promisify(execFile);
const time = (ms) =>
  `${[Math.floor(ms / 3600000), Math.floor(ms / 60000) % 60, Math.floor(ms / 1000) % 60].map((n) => String(n).padStart(2, '0')).join(':')},${String(ms % 1000).padStart(3, '0')}`;
async function identity(path) {
  const bytes = await readFile(path);
  const info = await stat(path, { bigint: true });
  let hash = 0xcbf29ce484222325n;
  for (const offset of [
    0,
    Math.floor(bytes.length / 2),
    Math.max(0, bytes.length - 65536),
  ]) {
    for (const byte of bytes.subarray(offset, offset + 65536))
      hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100000001b3n);
  }
  return {
    size: bytes.length,
    modified: `${info.mtimeNs / 1000000000n}.${String(info.mtimeNs % 1000000000n).padStart(9, '0')}`,
    sampleHash: hash.toString(16).padStart(16, '0'),
  };
}
async function prepare(root, context) {
  const small = join(root, 'scene-small.mkv'),
    large = join(root, 'scene-large.mkv');
  const inputs = [
    ['black', 1],
    ['red', 3],
    ['green', 2],
    ['blue', 2],
    ['white', 4],
  ].flatMap(([color, duration]) => [
    '-f',
    'lavfi',
    '-i',
    `color=c=${color}:s=160x90:r=24:d=${duration}`,
  ]);
  await run(
    process.env.VSF_FFMPEG ?? 'ffmpeg',
    [
      '-v',
      'error',
      '-nostdin',
      ...inputs,
      '-filter_complex',
      '[0:v][1:v][2:v][3:v][4:v]concat=n=5:v=1:a=0[out]',
      '-map',
      '[out]',
      '-c:v',
      'libx264',
      '-preset',
      'ultrafast',
      '-threads',
      '1',
      small,
    ],
    { timeout: 30000 },
  );
  await run(
    process.env.VSF_FFMPEG ?? 'ffmpeg',
    [
      '-v',
      'error',
      '-nostdin',
      '-f',
      'lavfi',
      '-i',
      "nullsrc=s=64x64:r=5/7,geq=lum='16+219*mod(N,2)':cb=128:cr=128",
      '-frames:v',
      '10001',
      '-c:v',
      'libx264',
      '-preset',
      'ultrafast',
      '-g',
      '1',
      '-threads',
      '1',
      large,
    ],
    { timeout: 60000 },
  );
  const source =
    '\uFEFF1\r\n00:00:01,000 --> 00:00:03,000\r\n{\\i1}First é 😀\r\n\r\n2\r\n00:00:04,050 --> 00:00:06,050\r\nSecond\r\n\r\n3\r\n00:00:08,000 --> 00:00:10,000\r\n{\\k20}Timed\r\n\r\n';
  await writeFile(join(root, 'scene-small.srt'), source);
  const text =
    '{\\c&H00FFFF&}<i>This is a subtitle line with Unicode 日本語 😀.</i>\nAnother line to wrap.';
  const big = Array.from(
    { length: 10000 },
    (_, i) =>
      `${i + 1}\n${time(i * 1400 + 1350)} --> ${time(i * 1400 + 2250)}\n${text}\n`,
  ).join('\n');
  await writeFile(join(root, 'scene-large.srt'), big);
  for (const [name, path, cuts] of [
    ['small', small, [1, 4, 6, 8]],
    ['large', large, Array.from({ length: 10000 }, (_, i) => (i + 1) * 1.4)],
  ]) {
    const profile = structuredClone(context.profile);
    profile.id = `scene-desktop-${name}`;
    profile.name = `Desktop scene ${name}`;
    profile.revision = 1;
    profile.timing = { cueGapMs: 80, sceneBeforeMs: 130, sceneAfterMs: 130 };
    profile.rules.duration = {
      enabled: true,
      minimumMs: name === 'large' ? 500 : 800,
      maximumMs: 7000,
    };
    profile.rules.speed = {
      enabled: name === 'large',
      minimumRs: null,
      maximumRsExclusive: 500,
      minimumCps: null,
      maximumCps: 150,
    };
    const mediaIdentity = await identity(path);
    await writeFile(
      join(root, `scene-${name}.vsfproject`),
      JSON.stringify({
        version: 1,
        mediaPath: path,
        subtitlePath: join(root, `scene-${name}.srt`),
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

export async function sceneQuality({
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
    until('scene workspace ready', () =>
      browser.$('button=Repair scene timing…').then((b) => b.isEnabled()),
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
    const id = await invoke('scene_repair_start', {
      session: state.session,
      generation: doc.generation,
      revision: doc.revision,
    });
    const view = await until('completed scene IPC proposal', async () => {
      const v = await invoke('scene_repair_view', {
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
  await until('small scene project', async () => {
    const d = await status();
    return d.cues.length === 3 && d.cues[0].text.includes('First é');
  });
  await ready();
  let checked = await until('scene findings', async () => {
    const v = await quality();
    return v.phase === 'ready' && v.sceneCoverage === 'checked' && v;
  });
  assert.equal(checked.findingsCount, 4);
  const before = await status();
  // Actual Problems filtering and draft-protected navigation.
  const problemButton = await browser.$(
    '//button[starts-with(normalize-space(.), "Problems")]',
  );
  if (!(await browser.$('[aria-label="Subtitle Problems"]').isExisting()))
    await problemButton.click();
  await until(
    'scene Problems rows',
    async () =>
      (await browser.$$('[aria-label="Subtitle Problems"] .finding')).length >
      0,
  );
  await browser.execute(() => {
    const s = document.querySelector('[aria-label="Problem rule"]');
    s.value = 'scene_end';
    s.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await until(
    'scene end filter',
    async () =>
      (await browser.$$('[aria-label="Subtitle Problems"] .finding')).length ===
      1,
  );
  const text = await browser.$('[aria-label="Subtitle text"]');
  await text.setValue('Preserve scene navigation draft');
  await (await browser.$('button=Cue 2 · End near scene cut')).click();
  await until('protected scene navigation', () =>
    browser.$('button=Cancel navigation').then((b) => b.isExisting()),
  );
  await (await browser.$('button=Cancel navigation')).click();
  assert.equal(await text.getValue(), 'Preserve scene navigation draft');
  assert.deepEqual(await status(), before);
  await (await browser.$('button=Revert draft')).click();
  await (await browser.$('button=Close Problems')).click();
  await (await browser.$('button=Repair scene timing…')).click();
  let dialog = await browser.$('dialog[aria-labelledby="scene-repair-title"]');
  await until('scene dialog', () => dialog.isDisplayed());
  await (await dialog.$('button=Create scene preview')).click();
  await until('scene preview rows', async () =>
    (await dialog.getText()).includes('2 safe changes'),
  );
  assert.match(await dialog.getText(), /timing review/i);
  assert.deepEqual(await status(), before);
  await browser.saveScreenshot(join(evidence, 'scene-preview.png'));
  await (await dialog.$('button=Accept scene changes')).click();
  await ready();
  const accepted = await status();
  assert.equal(accepted.revision, before.revision + 1);
  assert.deepEqual(
    accepted.cues.map((c) => [c.startMs, c.endMs]),
    [
      [1130, 3000],
      [4130, 5870],
      [8000, 10000],
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
  await until('saved scene copy', async () => !(await status()).dirty);
  const expected = source
    .replace('00:00:01,000', '00:00:01,130')
    .replace('00:00:04,050', '00:00:04,130')
    .replace('00:00:06,050', '00:00:05,870');
  assert.equal(await readFile(join(root, 'scene-copy.srt'), 'utf8'), expected);
  assert.equal(await readFile(join(root, 'scene-small.srt'), 'utf8'), source);
  await menu('openSubtitles');
  await ready();
  assert.deepEqual((await status()).cues, accepted.cues);
  const old = await proposal();
  await menu('scanScenes');
  await until('scene scan invalidates completed preview', async () => {
    try {
      await invoke('scene_repair_view', { id: old.id, offset: 0, limit: 100 });
      return false;
    } catch {
      return true;
    }
  });
  await assert.rejects(() => invoke('scene_repair_accept', { id: old.id }));
  checked = await until(
    'actual completed FFmpeg scene scan',
    async () => {
      const v = await quality();
      return v.phase === 'ready' && v.sceneCoverage === 'checked' && v;
    },
    60000,
  );
  await record({
    step: 'real scene Problems navigation, preview, preserved tags, failed save, atomic Undo/Redo, exact save/reopen and rescan rejection',
    beforeFindings: 4,
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
    invoke('scene_repair_accept', { id: mediaOld.id }),
  );
  await scenePerformance({
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

async function scenePerformance({
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
    result.checking = await until('10k scene findings', async () => {
      const v = await quality('scene_start');
      return v.phase === 'ready' && v;
    });
    assert.equal(result.checking.filteredCount, 10000);
    delete result.checking;
    result.interaction = await browser.execute(
      async (doc, session) => {
        const invoke = window.__TAURI__.core.invoke;
        const id = await invoke('scene_repair_start', {
          session,
          generation: doc.generation,
          revision: doc.revision,
        });
        const input = document.querySelector('[aria-label="Subtitle text"]');
        const start = performance.now();
        input.value = 'Scene performance draft';
        input.dispatchEvent(new Event('input', { bubbles: true }));
        await new Promise((r) => requestAnimationFrame(r));
        const typingMs = performance.now() - start,
          cancelStart = performance.now();
        await invoke('scene_repair_cancel', { id });
        let view;
        do {
          view = await invoke('scene_repair_view', {
            id,
            offset: 0,
            limit: 100,
          });
          if (performance.now() - cancelStart > 1000)
            throw new Error('Scene cancellation exceeded budget');
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
    await (await browser.$('button=Repair scene timing…')).click();
    const dialog = await browser.$(
      'dialog[aria-labelledby="scene-repair-title"]',
    );
    await until('10k scene dialog', () => dialog.isDisplayed());
    const began = Date.now();
    await (await dialog.$('button=Create scene preview')).click();
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
    await browser.saveScreenshot(join(evidence, 'scene-10000-preview.png'));
    const acceptStart = Date.now();
    await (await dialog.$('button=Accept scene changes')).click();
    await ready();
    result.acceptanceMs = Date.now() - acceptStart;
    assert.ok(
      result.acceptanceMs < result.budgetMs,
      'Scene acceptance exceeded budget',
    );
    const changed = await status();
    assert.equal(changed.revision, baseline.revision + 1);
    assert.equal(
      changed.cues.filter((c, i) => c.startMs !== baseline.cues[i].startMs)
        .length,
      10000,
    );
    assert.deepEqual(
      changed.cues.map((c) => [c.id, c.text, c.endMs]),
      baseline.cues.map((c) => [c.id, c.text, c.endMs]),
    );
    await (await browser.$('button=Undo edit')).click();
    await ready();
    assert.deepEqual((await status()).cues, baseline.cues);
    const captured = await proposal();
    const before = await status();
    await browser
      .$('[aria-label="Subtitle text"]')
      .then((e) => e.setValue('Invalidate scene proposal'));
    const invalidationStart = Date.now();
    await (await browser.$('button=Apply edit')).click();
    await until('10k scene invalidation', async () => {
      const v = await invoke('scene_repair_view', {
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
      step: 'real 10000-cue scene check, review, paging, full acceptance/Undo, cancellation, interaction and invalidation',
      ...result,
    });
  } catch (error) {
    result.status = 'failed';
    result.error = String(error.stack ?? error);
    throw error;
  } finally {
    await writeFile(
      join(evidence, 'scene-performance.json'),
      JSON.stringify(result, null, 2),
    );
  }
}
