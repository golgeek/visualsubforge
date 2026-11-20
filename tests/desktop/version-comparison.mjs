// Isolated real Rust/WebView review lane. All subtitles/media are generated here.
// File choices and DOM events are supplied; native-window observations are separate.
import { packageChoices, exchangeRoundTrip } from './review-packages.mjs';
import { acceptReview } from './review.mjs';
import { time } from './timing-fixtures.mjs';
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, writeFile, readFile, open, mkdtemp } from 'node:fs/promises';
import { createServer } from 'node:net';
import { once } from 'node:events';
import { resolve, join } from 'node:path';
import { parseArgs } from 'node:util';
import { remote } from 'webdriverio';
import { installEditorInput, commitCue } from './editor-input.mjs';
const { values } = parseArgs({
  options: {
    binary: { type: 'string' },
    packages: { type: 'boolean', default: false },
    'keyboard-undo': { type: 'boolean', default: false },
    root: { type: 'string' },
    hold: { type: 'boolean', default: false },
  },
});
assert(
  values.binary,
  '--binary must name an isolated desktop-e2e release build',
);
const binary = resolve(values.binary);
await mkdir(values.root ?? 'test-results', { recursive: true });
const root = await mkdtemp(
  join(resolve(values.root ?? 'test-results'), 'comparison-'),
);
const original = join(root, 'episode.srt'),
  copy = join(root, 'episode.reviewed.srt'),
  revised = join(root, 'received.srt'),
  project = join(root, 'review.vsfproject'),
  report = join(root, 'report.html'),
  video = join(root, 'synthetic.mp4');
const scene = (delta) =>
  Array.from(
    { length: 3 },
    (_, i) =>
      `${i + 5}\n${time(10500 + i * 1500 + delta)} --> ${time(11500 + i * 1500 + delta)}\nScene ${i + 1}\n\n`,
  ).join('');
const base =
  '1\n00:00:01,000 --> 00:00:04,000\none two three\n\n2\n00:00:05,000 --> 00:00:07,000\nsecond cue\n\n3\n00:00:08,000 --> 00:00:10,000\nunchanged\n\n' +
  scene(0);
await writeFile(original, base);
await writeFile(
  revised,
  '1\n00:00:01,000 --> 00:00:02,500\none\n\n2\n00:00:02,500 --> 00:00:04,000\ntwo three\n\n3\n00:00:05,240 --> 00:00:07,240\nsecond cue\n\n9\n00:00:07,350 --> 00:00:07,740\n(Door slams)\n\n4\n00:00:08,000 --> 00:00:10,000\nunchanged\n\n' +
    scene(400),
);
execFileSync(
  process.env.VSF_FFMPEG ?? 'ffmpeg',
  [
    '-v',
    'error',
    '-nostdin',
    '-f',
    'lavfi',
    '-i',
    'testsrc2=s=640x360:r=25:d=15',
    '-f',
    'lavfi',
    '-i',
    'sine=frequency=440:duration=15',
    '-c:v',
    'libx264',
    '-preset',
    'ultrafast',
    '-threads',
    '1',
    '-c:a',
    'aac',
    '-shortest',
    video,
  ],
  { timeout: 30000 },
);
const embedded = join(root, 'track.mkv'),
  extracted = join(root, 'track.srt'),
  reviewed = join(root, 'track.reviewed.srt');
const track = join(root, 'embedded.srt');
await writeFile(
  track,
  '1\n00:00:01,000 --> 00:00:02,000\nFirst line\n\n2\n00:00:02,050 --> 00:00:04,000\nSecond line\n\n3\n00:00:06,050 --> 00:00:08,000\nScene line\n',
);
execFileSync(process.env.VSF_FFMPEG ?? 'ffmpeg', [
  '-v',
  'error',
  '-f',
  'lavfi',
  '-i',
  'color=black:s=320x180:r=25:d=6',
  '-f',
  'lavfi',
  '-i',
  'color=white:s=320x180:r=25:d=4',
  '-i',
  track,
  '-filter_complex',
  '[0:v][1:v]concat=n=2:v=1:a=0[v]',
  '-map',
  '[v]',
  '-map',
  '2:s',
  '-c:v',
  'libx264',
  '-c:s',
  'srt',
  embedded,
]);
const choices = [
  { kind: 'subtitles', path: original },
  { kind: 'saveSubtitles', path: copy },
  { kind: 'saveProject', path: project },
  { kind: 'project', path: project },
  { kind: 'subtitles', path: revised },
  { kind: 'reviewOriginal', path: original },
  { kind: 'media', path: video },
  { kind: 'comparisonReport', path: report },
  { kind: 'extractSubtitles', path: embedded },
  { kind: 'saveSubtitles', path: extracted },
  { kind: 'saveSubtitles', path: reviewed },
];
for (const pair of ['chorus', 'ad-break']) {
  for (const [side, kind] of [
    ['original', 'reviewOriginal'],
    ['revised', 'reviewRevised'],
  ]) {
    const path = join(root, pair + '-' + side + '.srt');
    const fixture = new URL(
      '../fixtures/comparison/' + pair + '/' + side + '.srt',
      import.meta.url,
    );
    await writeFile(path, await readFile(fixture));
    choices.push({ kind, path });
  }
}
const server = createServer();
server.listen(0, '127.0.0.1');
await once(server, 'listening');
const port = server.address().port;
await new Promise((r) => server.close(r));
const log = await open(join(root, 'application.log'), 'wx');
const app = spawn(binary, [], {
  env: {
    ...process.env,
    VSF_E2E_ROOT: root,
    VSF_E2E_CHOICES: JSON.stringify(
      values.packages
        ? packageChoices(root, original, copy, video, project, report)
        : choices,
    ),
    TAURI_WEBDRIVER_PORT: String(port),
  },
  stdio: ['ignore', log.fd, log.fd],
});
let browser;
const receipt = {
  binary,
  root,
  steps: [],
  status: 'running',
  scope:
    'Real release WebView/Rust/files; supplied chooser results and DOM input. Generated media only.',
};
const save = () =>
  writeFile(
    join(root, 'receipt.json'),
    JSON.stringify(receipt, null, 2) + '\n',
  );
const delay = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(name, fn, timeout = 30000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const result = await fn();
    if (result) return result;
    await delay(50);
  }
  throw new Error('Timed out: ' + name);
}
async function invoke(command, args = {}) {
  const r = await browser.execute(
    async (name, input) => {
      try {
        return { value: await window.__TAURI__.core.invoke(name, input) };
      } catch (error) {
        return { error: String(error) };
      }
    },
    command,
    args,
  );
  assert(!r.error, r.error);
  return r.value;
}
const status = () =>
  invoke('document_action', {
    action: { kind: 'status' },
    generation: 0,
    revision: 0,
  });
async function menu(action) {
  await ready();
  await browser.execute(
    (action) => window.__TAURI__.event.emit('workspace-action', action),
    action,
  );
  await delay(100);
}
async function ready() {
  await until('workspace ready', () =>
    browser.execute(
      () =>
        document.querySelector('main')?.getAttribute('aria-busy') === 'false' &&
        !document.querySelector('dialog[open]'),
    ),
  );
}
async function click(text) {
  const ok = await browser.execute((text) => {
    const b = [...document.querySelectorAll('button')].find(
      (b) => b.textContent.trim() === text && !b.disabled,
    );
    b?.click();
    return !!b;
  }, text);
  assert(ok, 'Button unavailable: ' + text);
}
async function compared() {
  return until('comparison ready', async () => {
    const s = await status();
    const r = await invoke('review_action', {
      action: {
        kind: 'query',
        key: {
          generation: s.generation,
          revision: s.revision,
          epoch: s.review?.epoch ?? 0,
        },
        offset: 0,
        limit: 100,
        filter: null,
      },
    }).catch((error) => {
      if (String(error).includes('Review changed; refresh')) return null;
      throw error;
    });
    if (!r) return false;
    if (r.error) throw new Error(r.error);
    return !r.working && r.statistics && r;
  });
}
async function record(step) {
  receipt.steps.push(step);
  await save();
  console.log(step);
}
try {
  await until('driver', async () => {
    try {
      return (await (await fetch(`http://127.0.0.1:${port}/status`)).json())
        .value?.ready;
    } catch {
      return false;
    }
  });
  browser = await remote({
    hostname: '127.0.0.1',
    port,
    path: '/',
    capabilities: { browserName: 'wry' },
    logLevel: 'error',
    connectionRetryCount: 0,
    connectionRetryTimeout: 10000,
  });
  await browser.setTimeout({ script: 10000, implicit: 0 });
  await until(
    'document',
    async () => (await browser.getTitle()) === 'VisualSubForge',
  );
  await until('APIs', () =>
    browser.execute(
      () =>
        !!window.__TAURI__?.core &&
        !!document.querySelector('[aria-label="Subtitle text"]'),
    ),
  );
  await installEditorInput(browser);
  await ready();
  await menu('openSubtitles');
  await until('original open', async () => (await status()).path === original);
  await menu('startReview');
  await until('copy open', async () => (await status()).path === copy);
  assert.equal(await readFile(copy, 'utf8'), base);
  assert.equal((await compared()).statistics.changes, 0);
  await record(
    'Start Review writes byte-exact copy and opens an unchanged comparison without media',
  );
  await browser.$('[aria-label="Subtitle text"]').setValue('one revised three');
  if (!values.packages) await commitCue(browser);
  await until(
    'text edit',
    async () => (await status()).cues[0].text === 'one revised three',
  );
  assert.equal((await compared()).statistics.changes, 1);
  await until('singular change summary', () =>
    browser.execute(
      () =>
        document
          .querySelector('section[aria-label="Changes"] > p')
          ?.innerText.trim()
          .startsWith('1 change ·') === true,
    ),
  );
  if (values.packages) {
    await exchangeRoundTrip({
      keyboardUndo: values['keyboard-undo'],
      browser,
      root,
      project,
      report,
      menu,
      until,
      status,
      compared,
      record,
      invoke,
      ready,
    });
  } else {
    await comparisonWorkflows();
  }
  receipt.status = 'workflows-passed';
  await save();
  console.log('EVIDENCE ' + root);
  if (values.hold) {
    console.log(
      'Holding isolated app for independent native-window observation',
    );
    await delay(120000);
  }
  await menu('quit');
  await delay(150);
  if (await browser.execute(() => !!document.querySelector('dialog[open]')))
    await click('Discard changes');
  await until(
    'owned app exited',
    () => app.exitCode !== null || app.signalCode !== null,
  );
  assert.equal(app.exitCode, 0);
  receipt.exitCode = app.exitCode;
  receipt.status = 'passed';
  await save();
} catch (error) {
  receipt.status = 'failed';
  receipt.error = String(error);
  if (browser) {
    await browser.saveScreenshot(join(root, 'failure.png')).catch(() => {});
    receipt.dom = await browser
      .execute(() => document.body.innerText)
      .catch(() => null);
  }
  await save();
  throw error;
} finally {
  if (browser) await browser.deleteSession().catch(() => {});
  if (app.exitCode === null && app.signalCode === null) app.kill('SIGTERM');
  await log.close();
}

async function comparisonWorkflows() {
  await menu('saveProject');
  await until('saved project', async () => {
    try {
      return JSON.parse(await readFile(project, 'utf8')).review;
    } catch {
      return false;
    }
  });
  const persisted = JSON.parse(await readFile(project, 'utf8'));
  assert.equal(persisted.version, 4);
  assert(persisted.review);
  await record(
    'Reviewer edit and project v4 save retain original identity and options',
  );
  await menu('openProject');
  await until('reopened review', async () => {
    const s = await status();
    return s.path === copy && s.review?.path === original && !s.review.dirty;
  });
  assert.equal((await compared()).statistics.changes, 1);
  await record('Saved subtitles-only review project reopens');
  await menu('openSubtitles');
  await until('received open', async () => (await status()).path === revised);
  await menu('compareOriginal');
  const changes = await compared();
  assert.deepEqual(
    changes.rows.map((c) => c.kind),
    ['split', 'timing', 'added', 'timing'],
  );
  await record('Author attaches original and sees split plus timing changes');
  await menu('openMedia');
  await until(
    'media loaded',
    async () =>
      (await invoke('media_action', { action: { kind: 'status' }, session: 0 }))
        .loaded,
  );
  await until('ghosts', () =>
    browser.execute(
      () => document.querySelectorAll('[data-preview-cue]').length > 0,
    ),
  );
  await menu('nextChange');
  await until('Original line', () =>
    browser.execute(() => !!document.querySelector('[aria-label="Original"]')),
  );
  await browser.saveScreenshot(join(root, 'author-with-ghosts.png'));
  await record(
    'Opening generated media adds waveform ghosts and Original line',
  );
  await browser.execute(() =>
    [...document.querySelectorAll('button')]
      .find((b) => b.textContent.trim().startsWith('compared with'))
      ?.click(),
  );
  await until('Changes visible', () =>
    browser.execute(
      () => !!document.querySelector('[aria-label="Changes"] article'),
    ),
  );
  const panel = browser.$('section[aria-label="Changes"]');
  await (await panel.$('button=Split')).click();
  await until('split detail and fixed Revert header', () =>
    browser.execute(() => {
      const original = document.querySelector('[aria-label="Original"]');
      const header = original
        ?.querySelector('header button')
        ?.getBoundingClientRect();
      return (
        !!header &&
        header.bottom <= original.getBoundingClientRect().bottom + 1 &&
        original.innerText.includes('Group span')
      );
    }),
  );
  await browser.saveScreenshot(join(root, 'split-detail.png'));
  await (await browser.$('[data-cue-id="5"]')).click();
  await menu('previousChange');
  await until('previous from unchanged cue', () =>
    browser.execute(() =>
      [
        ...document.querySelectorAll(
          'section[aria-label="Changes"] article button[aria-pressed="true"]',
        ),
      ].some((b) => b.innerText === 'Added'),
    ),
  );
  await (await panel.$('nav').$('button=Timing')).click();
  await until('Timing filter excludes insertion and split', () =>
    panel.$('button=Revert 4 Timing Changes').isExisting(),
  );
  await (await panel.$('button=Expand')).click();
  await until(
    'expanded individual changes',
    async () => (await panel.$$('article')).length === 4,
  );
  await (await panel.$('button=Collapse')).click();
  await (await panel.$('button*=Shifted 3 cues')).click();
  await until(
    'selected block',
    async () =>
      (await browser.$$('[data-cue-id][aria-pressed="true"]')).length === 3,
  );
  await browser.saveScreenshot(join(root, 'shift-block.png'));
  await (await panel.$('button=Revert Block')).click();
  await until('block reverted', async () => (await compared()).matched === 3);
  await click('Undo');
  await until('block restored', async () => (await compared()).matched === 6);
  await (await panel.$('button=Revert Block')).click();
  await until(
    'second block revert',
    async () => (await compared()).matched === 3,
  );
  await (await panel.$('nav').$('button=All')).click();
  await until('insertion visible', () => panel.$('button=Added').isExisting());
  await record(
    'Unrelated insertion, kind filters, selected-cue navigation, group span, fixed Revert header and shift block Revert/Undo',
  );
  await menu('play');
  await until(
    'playing',
    async () =>
      !(
        await invoke('media_action', { action: { kind: 'status' }, session: 0 })
      ).paused,
  );
  for (const kind of ['added', 'split', 'timing']) {
    await ready();
    await until(kind + ' Revert button', () =>
      browser.execute((kind) => {
        const article = [
          ...document.querySelectorAll('[aria-label="Changes"] article'),
        ].find(
          (a) => a.querySelector('button')?.textContent.toLowerCase() === kind,
        );
        const revert = [...(article?.querySelectorAll('button') ?? [])].find(
          (b) => b.textContent === 'Revert',
        );
        if (revert && !revert.disabled) {
          revert.click();
          return true;
        }
        return false;
      }, kind),
    );
    await until(
      kind + ' reverted',
      async () => !(await compared()).rows.some((c) => c.kind === kind),
    );
    await record('Author reverts ' + kind + ' during native playback');
  }
  assert.equal((await compared()).statistics.changes, 0);
  await until('Revert UI settled', () =>
    browser.execute(() => document.body.innerText.includes('No changes yet')),
  );
  receipt.undoBefore = await status();
  await click('Undo');
  await delay(300);
  receipt.undoAfter = await status();
  await save();
  await until(
    'Undo restored timing',
    async () => (await compared()).statistics.changes === 1,
  );
  await record('One Undo restores the timing change');
  await until('Report ready', () =>
    browser.execute(() =>
      [...document.querySelectorAll('button')].some(
        (b) =>
          b.textContent.trim() === 'Export Comparison Report…' && !b.disabled,
      ),
    ),
  );
  await click('Export Comparison Report…');
  await until('report disclosure', () =>
    browser.$('button=Continue to Save').isExisting(),
  );
  await browser.$('dialog[open] input[maxlength]').setValue('Report reviewer');
  await click('Continue to Save');
  await until('report file', async () => {
    try {
      return (await readFile(report, 'utf8')).includes('Subtitle comparison');
    } catch {
      return false;
    }
  });
  assert.equal(await readFile(original, 'utf8'), base);
  await record('HTML exported and original bytes remain unchanged');
  assert(
    await browser.execute(() => {
      const footer = document
        .querySelector('.editorfooter')
        ?.getBoundingClientRect();
      const editor = document.querySelector('.editor')?.getBoundingClientRect();
      return !!footer && !!editor && footer.bottom <= editor.bottom + 1;
    }),
    'Original line preserves visible editor footer',
  );
  await browser.saveScreenshot(join(root, 'review-final.png'));
  await menu('extractSubtitles');
  await until('track listing', () => browser.$('button=Extract').isEnabled());
  await click('Extract');
  await until('discard old review', () =>
    browser.$('dialog[open]').$('button=Discard changes').isExisting(),
  );
  await click('Discard changes');
  await until(
    'extracted document',
    async () => (await status()).extractedFrom?.media_path === embedded,
  );
  await menu('saveAs');
  await until(
    'saved extracted subtitles',
    async () => (await status()).path === extracted,
  );
  await menu('startReview');
  await until(
    'reviewed extracted copy',
    async () => (await status()).path === reviewed,
  );
  const initial = (await status()).cues;
  await menu('scanScenes');
  await menu('fixGaps');
  await acceptReview(browser, status, until);
  const afterGaps = (await status()).cues;
  assert.notDeepEqual(afterGaps, initial);
  await menu('snapShots');
  await acceptReview(browser, status, until);
  const final = (await status()).cues;
  assert.notDeepEqual(final, afterGaps);
  const expected = final.filter(
    (c, i) => c.startMs !== initial[i].startMs || c.endMs !== initial[i].endMs,
  );
  const result = await compared();
  assert.deepEqual(
    result.rows.map((c) => [
      c.kind,
      c.revised[0].cue_id,
      c.revised[0].start_ms,
      c.revised[0].end_ms,
    ]),
    expected.map((c) => ['timing', c.id, c.startMs, c.endMs]),
  );
  await browser.execute(() =>
    [...document.querySelectorAll('button')]
      .find((b) => b.textContent.trim().startsWith('compared with'))
      ?.click(),
  );
  await until(
    'combined Changes rows',
    async () =>
      (await browser.execute(
        () =>
          [
            ...document.querySelectorAll(
              'section[aria-label="Changes"] article button',
            ),
          ].filter((b) => b.innerText === 'Timing').length,
      )) === expected.length,
  );
  await browser.saveScreenshot(join(root, 'extracted-timing.png'));
  await record(
    'Extracted generated MKV track, Save As, Start Review, Fix Gaps and Snap to Scene Changes; exact timing rows match both edits',
  );
  for (const remaining of [1, 0]) {
    await click('Undo');
    await until(
      'undo combined timing change',
      async () => (await compared()).matched === remaining,
    );
  }
  await menu('shiftCues');
  const offset = await browser.$('[aria-label="Shift offset in milliseconds"]');
  await until('shift controls', () => offset.isExisting());
  await offset.setValue('400');
  await until('shift proposal for the three scene cues', () =>
    browser
      .$('.timing-review .apply')
      .getText()
      .then((t) => t === 'Apply 3 changes'),
  );
  await acceptReview(browser, status, until);
  await until('shifted scene forms one block', async () => {
    const r = await compared();
    return r.matched === 3 && r.blocks?.[0]?.block.delta_ms === 400;
  });
  await browser.execute(() =>
    [...document.querySelectorAll('button')]
      .find((b) => b.textContent.trim().startsWith('compared with'))
      ?.click(),
  );
  await until('block control', () =>
    browser.$('button=Revert Block').isExisting(),
  );
  await click('Revert Block');
  await until(
    'scene shift reverted',
    async () => (await compared()).matched === 0,
  );
  await click('Undo');
  await until(
    'Undo restores complete shifted scene',
    async () => (await compared()).blocks?.[0]?.block.delta_ms === 400,
  );
  await record(
    'Shifted three scene cues by +400 ms in Shift / Align Cues, reverted block, restored it with one Undo',
  );
  async function openPair(pair) {
    await menu('compareFiles');
    await until('discard previous review', () =>
      browser.$('dialog[open]').$('button=Discard changes').isExisting(),
    );
    await click('Discard changes');
    await until(
      'opened ' + pair,
      async () =>
        (await status()).review?.path === join(root, pair + '-original.srt'),
    );
    await compared();
  }
  await openPair('chorus');
  await until('expand surviving chorus', () =>
    panel.$('button=Expand').isExisting(),
  );
  await (await panel.$('button=Expand')).click();
  await until(
    'expanded chorus rows',
    async () => (await panel.$$('article')).length === 9,
  );
  const chorus = await browser.execute(() =>
    Array.from(
      document.querySelectorAll('section[aria-label="Changes"] article'),
      (row) => ({
        kind: row.querySelector('button[aria-pressed]')?.innerText,
        times: row.querySelector('small')?.innerText,
      }),
    ),
  );
  assert.deepEqual(
    chorus.filter((row) => row.kind === 'Removed').map((row) => row.times),
    ['In 00:00:04,000 · Out 00:00:05,500'],
  );
  assert.equal(
    chorus.filter(
      (row) =>
        row.kind === 'Timing' && row.times.match(/\+0\.300 s/g)?.length === 2,
    ).length,
    8,
  );
  const repeated = (await panel.$$('article'))[3];
  await (await repeated.$('button=Timing')).click();
  await until('Original shows the confirmed chorus offset', () =>
    browser.execute(() =>
      document
        .querySelector('[aria-label="Original"]')
        ?.innerText.includes('+0.300 s'),
    ),
  );
  await browser.saveScreenshot(join(root, 'chorus.png'));
  await record(
    'Expanded repeated chorus: removal at 00:04.000, eight +0.300 s pairs, selected cue Original shows +0.300 s',
  );
  await openPair('ad-break');
  const adLabel = 'Shifted 4 cues by +30.000 s (00:00:38,000 – 00:00:45,500)';
  await until('revised ad-break span', () =>
    panel.$('button=' + adLabel).isExisting(),
  );
  assert.equal((await panel.$$('article')).length, 1);
  await (await panel.$('button=' + adLabel)).click();
  await browser.saveScreenshot(join(root, 'ad-break.png'));
  await click('Revert Block');
  await until(
    'ad break reverted',
    async () => (await compared()).statistics.changes === 0,
  );
  await until('no visible changes', () =>
    browser.execute(() =>
      document
        .querySelector('section[aria-label="Changes"]')
        ?.innerText.includes('No changes yet'),
    ),
  );
  await click('Undo');
  await until('restored +30 s block', () =>
    panel.$('button=' + adLabel).isExisting(),
  );
  await record(
    'One +30 s ad-break block on the revised timeline; Revert Block cleared all four changes and one Undo restored it',
  );
}
