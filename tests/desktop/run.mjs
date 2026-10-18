// Real webview + Tauri IPC + filesystem. Only file selection and app-data paths
// are supplied by the optional desktop-e2e build; document commands are not mocked.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import {
  open,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  writeFile,
} from 'node:fs/promises';
import { createServer } from 'node:net';
import { join, resolve, toNamespacedPath } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { parseArgs } from 'node:util';
import { remote } from 'webdriverio';
import { qualitySettings } from './quality.mjs';
import { qualityChecking } from './checks.mjs';
import { gapReview } from './gaps.mjs';
import { textEditing } from './text.mjs';
import { textPerformance } from './text-performance.mjs';
import { qualityPerformance } from './performance.mjs';
import { sceneQuality } from './scene-quality.mjs';
import { durationQuality } from './duration-quality.mjs';
import { statisticsWorkflow } from './statistics.mjs';
import { assDocuments } from './ass-documents.mjs';
import { assPerformance } from './ass-performance.mjs';
import { batchTiming } from './batch-timing.mjs';
import { styledResync } from './resync.mjs';
import { stylePerformance } from './style-performance.mjs';
import { styleAuthoring } from './style-authoring.mjs';
import { fontAuthoring } from './font-authoring.mjs';
import { fontPerformance } from './font-performance.mjs';
import { languageTools } from './language.mjs';
import { languagePerformance } from './language-performance.mjs';
import { conversionWorkflow } from './conversion.mjs';
import { conversionPerformance } from './conversion-performance.mjs';
import { webvttWorkflow } from './webvtt.mjs';
import { webvttPerformance } from './webvtt-performance.mjs';
import { legacyImportWorkflow } from './legacy-import.mjs';
import { legacyImportPerformance } from './legacy-import-performance.mjs';
import { translationWorkflow } from './translation.mjs';
import { translationPerformance } from './translation-performance.mjs';
import { pluginWorkflow } from './plugins.mjs';
import { pluginPerformance } from './plugin-performance.mjs';

const { values } = parseArgs({
  options: {
    binary: { type: 'string' },
    cli: { type: 'string' },
    performance: { type: 'boolean', default: false },
    scenes: { type: 'boolean', default: false },
    duration: { type: 'boolean', default: false },
    statistics: { type: 'boolean', default: false },
    ass: { type: 'boolean', default: false },
    batch: { type: 'boolean', default: false },
    resync: { type: 'boolean', default: false },
    language: { type: 'boolean', default: false },
    styles: { type: 'boolean', default: false },
    fonts: { type: 'boolean', default: false },
    conversion: { type: 'boolean', default: false },
    webvtt: { type: 'boolean', default: false },
    translation: { type: 'boolean', default: false },
    plugins: { type: 'boolean', default: false },
    'plugins-editor': { type: 'boolean', default: false },
    'plugin-performance': { type: 'boolean', default: false },
    'translation-performance': { type: 'boolean', default: false },
    'legacy-import': { type: 'boolean', default: false },
    'legacy-import-performance': { type: 'boolean', default: false },
    'webvtt-performance': { type: 'boolean', default: false },
    'conversion-performance': { type: 'boolean', default: false },
    'font-performance': { type: 'boolean', default: false },
    'style-performance': { type: 'boolean', default: false },
    'language-performance': { type: 'boolean', default: false },
    'ass-performance': { type: 'boolean', default: false },
  },
});
if (values.plugins && values['plugins-editor'])
  throw new Error('Choose --plugins or --plugins-editor, not both.');
const cli = resolve(
  values.cli ?? `target/debug/vsf${process.platform === 'win32' ? '.exe' : ''}`,
);
const binary = resolve(
  values.binary ??
    `target/debug/visualsubforge${process.platform === 'win32' ? '.exe' : ''}`,
);
await mkdir('test-results', { recursive: true });
const evidence = await realpath(
  await mkdtemp(resolve('test-results/desktop-')),
);
const root = join(evidence, 'data');
await mkdir(root);
const source = join(root, 'source.srt');
const copy = join(root, 'copy.srt');
const recoveredCopy = join(root, 'recovered.srt');
const invalid = join(root, 'missing-parent', 'copy.srt');
const original =
  '\uFEFF7\r\n00:00:01,000 --> 00:00:02,000\r\n<i>世界</i>\r\n\r\n9\r\n00:00:02,120 --> 00:00:03,000\r\nSecond\r\n\r\n';
await writeFile(source, original, { flag: 'wx' });
await writeFile(join(root, 'invalid.vsfpreset'), '{invalid', { flag: 'wx' });
await writeFile(
  join(root, 'timing.vsfpreset'),
  JSON.stringify({
    version: 1,
    name: 'Timing only',
    timing: {
      enabled: true,
      snapCues: true,
      snapScenes: true,
      cueGapMs: 167,
      sceneBeforeMs: 120,
      sceneAfterMs: 120,
      snapDistancePx: 8,
    },
  }),
  { flag: 'wx' },
);
const receipt = {
  binary,
  platform: process.platform,
  architecture: process.arch,
  evidence,
  steps: [],
};
let app;
let browser;
let log;
let exit;
let launchCount = 0;
const runDeadline =
  Date.now() +
  (values.ass ||
  values.batch ||
  values.resync ||
  values.language ||
  values.styles ||
  values.fonts ||
  values.conversion ||
  values.webvtt ||
  values.translation ||
  values.plugins ||
  values['plugins-editor'] ||
  values['plugin-performance'] ||
  values['translation-performance'] ||
  values['legacy-import'] ||
  values['legacy-import-performance'] ||
  values['webvtt-performance'] ||
  values['conversion-performance'] ||
  values['font-performance'] ||
  values['style-performance'] ||
  values['language-performance'] ||
  values['ass-performance']
    ? 900_000
    : values.scenes || values.duration || values.statistics
      ? 480_000
      : values.performance
        ? 300_000
        : 180_000);

async function record(step) {
  receipt.steps.push(step);
  await writeFile(
    join(evidence, 'receipt.json'),
    `${JSON.stringify(receipt, null, 2)}\n`,
  );
}
async function until(label, probe, timeout = 20_000) {
  const deadline = Math.min(Date.now() + timeout, runDeadline);
  while (Date.now() < deadline) {
    const value = await probe();
    if (value) return value;
    if (app && (app.exitCode !== null || app.signalCode !== null))
      throw new Error(`Application exited while waiting for ${label}`);
    await delay(100);
  }
  throw new Error(`Timed out waiting for ${label}`);
}
async function freePort() {
  const server = createServer();
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const port = server.address().port;
  await new Promise((done, reject) =>
    server.close((error) => (error ? reject(error) : done())),
  );
  return port;
}
async function launch(choices) {
  const port = await freePort();
  log = await open(join(evidence, `app-${++launchCount}.log`), 'wx');
  app = spawn(binary, [], {
    env: {
      ...process.env,
      VSF_E2E_ROOT: root,
      VSF_E2E_CHOICES: JSON.stringify(choices),
      TAURI_WEBDRIVER_PORT: String(port),
    },
    stdio: ['ignore', log.fd, log.fd],
  });
  exit = new Promise((done, reject) => {
    app.once('error', reject);
    app.once('exit', (code, signal) => done({ code, signal }));
  });
  // Keep a spawn error observed even if readiness fails first.
  void exit.catch(() => {});
  await until(
    'owned application WebDriver server',
    async () => {
      try {
        return (
          await (
            await fetch(`http://127.0.0.1:${port}/status`, {
              signal: AbortSignal.timeout(1000),
            })
          ).json()
        ).value?.ready;
      } catch {
        return false;
      }
    },
    30_000,
  );
  browser = await remote({
    hostname: '127.0.0.1',
    port,
    path: '/',
    capabilities: { browserName: 'wry' },
    logLevel: 'error',
    connectionRetryCount: 0,
    connectionRetryTimeout: 10_000,
  });
  await browser.setTimeout({ script: 10_000, implicit: 0 });
  // /status establishes a window, not completion of its initial navigation.
  // The embedded driver's execute/sync polls a page-global result; navigation
  // between those evaluations loses the result. getTitle is a single native
  // evaluation and safely observes the actual document before that first script.
  await until(
    'application document',
    async () => (await browser.getTitle()) === 'VisualSubForge',
  );
  await record({
    step: 'application document loaded',
    launch: launchCount,
    title: await browser.getTitle(),
    url: await browser.getUrl(),
  });
  await until('Tauri APIs and rendered editor', async () =>
    browser.execute(
      () =>
        !!window.__TAURI__?.core &&
        !!document.querySelector('[aria-label="Subtitle text"]'),
    ),
  );
  await until('ready workspace', async () =>
    (await browser.$('main'))
      .getAttribute('aria-busy')
      .then((value) => value === 'false'),
  );
  await record({ step: 'launch', pid: app.pid, port, mockedIpc: false });
}
async function invoke(command, args = {}) {
  const result = await browser.execute(
    async (name, input) => {
      try {
        return {
          ok: true,
          value: await window.__TAURI__.core.invoke(name, input),
        };
      } catch (error) {
        return { ok: false, error: String(error) };
      }
    },
    command,
    args,
  );
  assert.equal(result.ok, true, result.error);
  return result.value;
}
async function status() {
  return invoke('document_action', {
    action: { kind: 'status' },
    generation: 0,
    revision: 0,
  });
}
async function documentIs(label, predicate) {
  return until(label, async () => {
    const value = await status();
    return predicate(value) && value;
  });
}
function samePath(actual, expected) {
  // Rust canonicalizes Windows paths with the extended-length prefix. Keep
  // the complete path comparison, normalizing only that representation.
  return (
    typeof actual === 'string' &&
    toNamespacedPath(actual) === toNamespacedPath(expected)
  );
}
async function menu(action) {
  const result = await browser.execute(async (action) => {
    try {
      await window.__TAURI__.event.emit('workspace-action', action);
      return null;
    } catch (error) {
      return String(error);
    }
  }, action);
  assert.equal(result, null);
}
async function edit(text) {
  const input = await browser.$('[aria-label="Subtitle text"]');
  await input.setValue(text);
  const apply = await browser.$('button=Apply edit');
  await until('enabled Apply', () => apply.isEnabled());
  await apply.click();
  return documentIs(
    'accepted edit',
    (doc) => doc.cues[0]?.text === text && doc.dirty,
  );
}
async function terminate() {
  if (app && app.exitCode === null && app.signalCode === null) {
    app.kill('SIGKILL');
    await Promise.race([
      exit,
      delay(10_000, undefined, { ref: false }).then(() => {
        throw new Error('Owned application did not terminate');
      }),
    ]);
  }
  browser = undefined;
  await log?.close();
  log = undefined;
}

try {
  await launch([
    { kind: 'subtitles', path: source },
    { kind: 'qualityPreset', path: null },
    { kind: 'qualityPreset', path: join(root, 'invalid.vsfpreset') },
    { kind: 'saveQualityPreset', path: join(root, 'exported.vsfpreset') },
    { kind: 'qualityPreset', path: join(root, 'exported.vsfpreset') },
    { kind: 'timingPreset', path: join(root, 'timing.vsfpreset') },
    { kind: 'saveSubtitles', path: null },
    { kind: 'saveSubtitles', path: invalid },
    { kind: 'saveSubtitles', path: copy },
    { kind: 'subtitles', path: copy },
  ]);
  await menu('openSubtitles');
  let opened = await documentIs(
    'opened source',
    (doc) => doc.loaded && doc.cues.length === 2,
  );
  assert.equal(opened.cues[0].text, '<i>世界</i>');
  const persistedQuality = await qualitySettings({
    browser,
    menu,
    invoke,
    status,
    until,
    root,
    evidence,
    record,
  });
  await qualityChecking({
    browser,
    invoke,
    status,
    until,
    root,
    evidence,
    record,
    source,
    cli,
  });
  opened = await gapReview({ browser, status, until, evidence, record });
  opened = await textEditing({
    browser,
    status,
    until,
    evidence,
    record,
    root,
  });
  const changed = await edit('Edited 世界');
  assert.equal(changed.revision, opened.revision + 1);
  assert.deepEqual(
    changed.cues.map((cue) => cue.id),
    opened.cues.map((cue) => cue.id),
  );
  assert.equal(await readFile(source, 'utf8'), original);
  await (await browser.$('button=Undo edit')).click();
  const undone = await documentIs(
    'one Undo',
    (doc) => doc.cues[0]?.text === '<i>世界</i>',
  );
  assert.deepEqual(undone.cues, opened.cues);
  await (await browser.$('button=Redo edit')).click();
  await documentIs('one Redo', (doc) => doc.cues[0]?.text === 'Edited 世界');
  await record({
    step: 'real IPC edit and one Undo/Redo',
    generation: opened.generation,
  });

  await menu('saveAs'); // cancelled file selection
  await until('cancelled Save As returns to editor', async () =>
    (await browser.$('button=Undo edit')).isEnabled(),
  );
  assert.equal((await status()).dirty, true);
  await menu('saveAs'); // missing parent
  await until('failed save reported', async () =>
    (await browser.$('footer [role="status"]'))
      .getText()
      .then((text) => /Cannot|cannot|No such|not find/.test(text)),
  );
  const failed = await status();
  assert.equal(failed.dirty, true);
  assert.ok(
    samePath(failed.path, source),
    `Unexpected source path: ${failed.path}`,
  );
  assert.equal(await readFile(source, 'utf8'), original);
  await menu('saveAs');
  await documentIs(
    'successful Save As',
    (doc) => !doc.dirty && samePath(doc.path, copy),
  );
  const savedBytes = await readFile(copy, 'utf8');
  assert.equal(savedBytes, original.replace('<i>世界</i>', 'Edited 世界'));
  await menu('closeSubtitles');
  await documentIs('closed document', (doc) => !doc.loaded);
  await menu('openSubtitles');
  await documentIs(
    'reopened saved copy',
    (doc) => samePath(doc.path, copy) && doc.cues[0]?.text === 'Edited 世界',
  );
  await record({
    step: 'cancelled/failed Save As preservation and successful save/reopen',
  });

  await edit('Recovery 世界');
  await until('durable recovery checkpoint', async () => {
    for (const entry of await readdir(join(root, 'recovery-v1'), {
      withFileTypes: true,
    })) {
      if (!entry.isDirectory()) continue;
      try {
        if (
          (
            await readFile(
              join(root, 'recovery-v1', entry.name, 'snapshot.json'),
              'utf8',
            )
          ).includes('Recovery 世界')
        )
          return true;
      } catch (error) {
        if (error.code !== 'ENOENT') throw error;
      }
    }
    return false;
  });
  await terminate(); // abrupt loss, not ordinary close/discard
  await record({
    step: 'checkpoint durable before abrupt owned-process termination',
  });
  // An explicit Save As may reuse an unchanged ordinary recovery source.
  // A newer external edit must instead be protected by the saved-byte baseline.
  const externalBytes = savedBytes.replace(
    'Edited 世界',
    'External newer edit',
  );
  await writeFile(copy, externalBytes);
  await launch([
    { kind: 'saveSubtitles', path: copy },
    { kind: 'saveSubtitles', path: recoveredCopy },
    ...(values.performance
      ? [{ kind: 'subtitles', path: join(root, 'quality-10000.srt') }]
      : []),
    ...(values.scenes
      ? [
          { kind: 'project', path: join(root, 'scene-small.vsfproject') },
          { kind: 'saveSubtitles', path: join(root, 'scene-copy.srt') },
          { kind: 'subtitles', path: join(root, 'scene-copy.srt') },
          { kind: 'project', path: join(root, 'scene-large.vsfproject') },
        ]
      : []),
    ...(values.statistics
      ? [
          { kind: 'subtitles', path: join(root, 'statistics-small.srt') },
          { kind: 'project', path: join(root, 'statistics-small.vsfproject') },
          { kind: 'project', path: join(root, 'statistics-large.vsfproject') },
        ]
      : []),
    ...(values.duration
      ? [
          { kind: 'project', path: join(root, 'duration-small.vsfproject') },
          { kind: 'saveSubtitles', path: join(root, 'duration-copy.srt') },
          { kind: 'subtitles', path: join(root, 'duration-copy.srt') },
          { kind: 'project', path: join(root, 'duration-large.vsfproject') },
        ]
      : []),
  ]);
  const recover = await browser.$('button=Recover');
  assert.deepEqual(
    (await invoke('quality_settings')).current.profile,
    persistedQuality,
  );
  await record({
    step: 'quality profile restored after actual process restart',
  });
  await until('recoverable checkpoint', () => recover.isEnabled());
  await recover.click();
  await documentIs(
    'recovered unsaved copy',
    (doc) => doc.requiresSaveAs && doc.cues[0]?.text === 'Recovery 世界',
  );
  await menu('saveAs');
  await until('recovered source conflict reported', async () =>
    (await browser.$('footer [role="status"]'))
      .getText()
      .then((text) => /changed on disk/.test(text)),
  );
  assert.equal((await status()).requiresSaveAs, true);
  assert.equal(await readFile(copy, 'utf8'), externalBytes);
  await menu('saveAs');
  await documentIs(
    'saved recovered copy',
    (doc) =>
      !doc.requiresSaveAs && !doc.dirty && samePath(doc.path, recoveredCopy),
  );
  assert.equal(
    await readFile(recoveredCopy, 'utf8'),
    original.replace('<i>世界</i>', 'Recovery 世界'),
  );
  assert.equal(await readFile(copy, 'utf8'), externalBytes);
  assert.equal(await readFile(source, 'utf8'), original);
  await browser.saveScreenshot(join(evidence, 'recovered-saved.png'));
  await record({
    step: 'real recovery UI, external conflict and separate Save As',
  });
  if (values.performance) {
    await qualityPerformance({
      browser,
      menu,
      invoke,
      status,
      until,
      root,
      evidence,
      record,
      binary,
      pid: app.pid,
    });
    await textPerformance({ browser, status, until, evidence, record });
  }
  if (values.scenes) {
    await sceneQuality({
      browser,
      menu,
      invoke,
      status,
      until,
      root,
      evidence,
      record,
    });
  }
  if (values.statistics)
    await statisticsWorkflow({
      browser,
      menu,
      invoke,
      status,
      until,
      root,
      evidence,
      record,
      cli,
    });
  if (values.duration)
    await durationQuality({
      browser,
      menu,
      invoke,
      status,
      until,
      root,
      evidence,
      record,
    });
  if (values['legacy-import'])
    await legacyImportWorkflow({
      menu,
      invoke,
      status,
      until,
      root,
      evidence,
      record,
      binary,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values.translation)
    await translationWorkflow({
      menu,
      invoke,
      status,
      until,
      root,
      evidence,
      record,
      binary,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values['plugin-performance'])
    await pluginPerformance({
      status,
      until,
      root,
      evidence,
      record,
      binary,
      getPid: () => app.pid,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values.plugins || values['plugins-editor'])
    await pluginWorkflow({
      samePath,
      withNativePreview: values.plugins,
      menu,
      invoke,
      status,
      until,
      root,
      evidence,
      record,
      binary,
      getPid: () => app.pid,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values.webvtt)
    await webvttWorkflow({
      menu,
      invoke,
      status,
      until,
      root,
      evidence,
      record,
      binary,
      cli,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values['translation-performance'])
    await translationPerformance({
      status,
      until,
      root,
      evidence,
      record,
      binary,
      getPid: () => app.pid,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values['legacy-import-performance'])
    await legacyImportPerformance({
      status,
      until,
      root,
      evidence,
      record,
      binary,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values['webvtt-performance'])
    await webvttPerformance({
      status,
      until,
      root,
      evidence,
      record,
      binary,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values.ass)
    await assDocuments({
      menu,
      invoke,
      status,
      until,
      root,
      evidence,
      record,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values.batch)
    await batchTiming({
      root,
      evidence,
      menu,
      invoke,
      status,
      until,
      record,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values.resync)
    await styledResync({
      samePath,
      root,
      evidence,
      menu,
      invoke,
      status,
      until,
      record,
      cli,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values['style-performance'])
    await stylePerformance({
      root,
      evidence,
      binary,
      record,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values['font-performance'])
    await fontPerformance({
      root,
      evidence,
      binary,
      record,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values.fonts)
    await fontAuthoring({
      samePath,
      root,
      evidence,
      menu,
      invoke,
      status,
      until,
      record,
      binary,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values.conversion)
    await conversionWorkflow({
      samePath,
      root,
      evidence,
      menu,
      invoke,
      status,
      until,
      record,
      binary,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values['conversion-performance'])
    await conversionPerformance({
      root,
      evidence,
      binary,
      record,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values.styles)
    await styleAuthoring({
      samePath,
      root,
      evidence,
      menu,
      invoke,
      status,
      until,
      record,
      binary,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values.language)
    await languageTools({
      samePath,
      root,
      evidence,
      menu,
      invoke,
      status,
      until,
      record,
      binary,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values['language-performance'])
    await languagePerformance({
      status,
      invoke,
      menu,
      until,
      root,
      evidence,
      record,
      binary,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  if (values['ass-performance'] || values.batch)
    await assPerformance({
      batch: values.batch,
      status,
      root,
      evidence,
      record,
      binary,
      restart: async (choices) => {
        await terminate();
        await launch(choices);
        return browser;
      },
    });
  await menu('quit').catch(() => {}); // app can exit before the event ACK reaches WebDriver
  const stopped = await Promise.race([
    exit,
    delay(15_000, undefined, { ref: false }).then(() => {
      throw new Error('Normal application quit timed out');
    }),
  ]);
  assert.equal(stopped.code, 0);
  await record({ step: 'normal application shutdown', ...stopped });
  receipt.status = 'passed';
  console.log(
    `PASS: real desktop IPC, history, saves and recovery. Evidence: ${evidence}`,
  );
} catch (error) {
  receipt.status = 'failed';
  receipt.error = String(error.stack ?? error);
  if (browser) {
    await browser.saveScreenshot(join(evidence, 'failure.png')).catch(() => {});
    await writeFile(
      join(evidence, 'failure.html'),
      await browser.getPageSource().catch(() => ''),
    ).catch(() => {});
  }
  console.error(`FAIL: ${error.stack ?? error}\nEvidence: ${evidence}`);
  process.exitCode = 1;
} finally {
  await terminate();
  await writeFile(
    join(evidence, 'receipt.json'),
    `${JSON.stringify(receipt, null, 2)}\n`,
  );
}
