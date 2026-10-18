// Actual editor/media worker and process-only audio capture. Supplied file
// choices and typed IPC do not establish native pointer or speaker acceptance.
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { promisify } from 'node:util';

const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');

function fixture(seed) {
  const frames = 4 * 48000;
  const bytes = Buffer.alloc(44 + frames * 4);
  bytes.write('RIFF');
  bytes.writeUInt32LE(bytes.length - 8, 4);
  bytes.write('WAVEfmt ', 8);
  bytes.writeUInt32LE(16, 16);
  bytes.writeUInt16LE(1, 20);
  bytes.writeUInt16LE(2, 22);
  bytes.writeUInt32LE(48000, 24);
  bytes.writeUInt32LE(48000 * 4, 28);
  bytes.writeUInt16LE(4, 32);
  bytes.writeUInt16LE(16, 34);
  bytes.write('data', 36);
  bytes.writeUInt32LE(frames * 4, 40);
  for (let channel = 0; channel < 2; channel++) {
    let state = (seed + channel * 65537) >>> 0;
    const noise = new Float64Array(frames + 4);
    for (let i = 0; i < noise.length; i++) {
      state ^= state << 13;
      state ^= state >>> 17;
      state ^= state << 5;
      noise[i] = (state >>> 0) / 2147483648 - 1;
    }
    for (let i = 0; i < frames; i++) {
      const sample = Math.round(
        ((noise[i] +
          2 * noise[i + 1] +
          3 * noise[i + 2] +
          2 * noise[i + 3] +
          noise[i + 4]) *
          8000) /
          9,
      );
      bytes.writeInt16LE(sample, 44 + i * 4 + channel * 2);
    }
  }
  return bytes;
}

export async function nativeAudioWorkflow({
  restart,
  root,
  evidence,
  menu,
  invoke,
  status,
  until,
  record,
  binary,
  captureOnly = false,
}) {
  assert.ok(
    ['darwin', 'linux'].includes(process.platform),
    'This capture lane requires macOS or Linux with a Pulse stream monitor.',
  );
  assert.ok(
    process.env.VSF_PROCESS_AUDIO_CAPTURE,
    'Set VSF_PROCESS_AUDIO_CAPTURE.',
  );
  if (!captureOnly)
    assert.ok(
      process.env.VSF_AUDIO_ANALYSIS_PYTHON,
      'Set VSF_AUDIO_ANALYSIS_PYTHON to a Python with NumPy.',
    );
  assert.ok(
    process.env.VSF_EXPECT_MPV,
    'Set VSF_EXPECT_MPV to the selected runtime library.',
  );
  const helper = resolve(process.env.VSF_PROCESS_AUDIO_CAPTURE);
  const source = join(root, 'native-audio-original.wav');
  const replacement = join(root, 'native-audio-replacement.wav');
  const subtitles = join(root, 'native-audio.srt');
  await writeFile(source, fixture(505019), { flag: 'wx' });
  await writeFile(replacement, fixture(505020), { flag: 'wx' });
  await writeFile(
    subtitles,
    '1\n00:00:01,000 --> 00:00:02,000\nKeep this document\n\n',
    { flag: 'wx' },
  );
  const report = {
    status: 'running',
    captureKind:
      process.platform === 'linux'
        ? 'pulse-single-stream-receive-time'
        : 'macos-process-tap',
    binary,
    binarySha256: hash(await readFile(binary)),
    helper,
    helperSha256: hash(await readFile(helper)),
    testInputs: Object.fromEntries(
      await Promise.all(
        [
          'tests/desktop/native-audio.mjs',
          'tests/desktop/run.mjs',
          'tests/native/audio-editor-content.py',
          'tests/native/audio-fingerprint.py',
          'tests/native/audio-loop-content.py',
        ].map(async (path) => [resolve(path), hash(await readFile(path))]),
      ),
    ),
    scope:
      'Actual editor typed IPC/media worker, native process-only PCM and source preservation. Supplied file choices; capture analysis is a separate required step. No physical input, device presentation or speaker claim.',
    sources: Object.fromEntries(
      await Promise.all(
        [source, replacement, subtitles].map(async (p) => [
          p,
          hash(await readFile(p)),
        ]),
      ),
    ),
    captures: [],
    observations: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'native-audio.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const { browser, pid } = await restart([
    { kind: 'subtitles', path: subtitles },
    { kind: 'media', path: source },
    { kind: 'media', path: replacement },
  ]);
  report.pid = pid;
  const media = () =>
    invoke('media_action', { action: { kind: 'status' }, session: 0 });
  let session;
  const act = (action) => invoke('media_action', { action, session });
  const seek = async (seconds) => {
    const observation = { name: 'paused-seek', seconds, rows: [] };
    report.observations.push(observation);
    try {
      await act({ kind: 'pause', paused: true });
      await act({ kind: 'seek', seconds });
      return await until(`paused exact seek ${seconds}`, async () => {
        const m = await media();
        observation.rows.push(m);
        assert.equal(m.error, null);
        return (
          m.session === session &&
          m.paused &&
          !m.seeking &&
          m.positionSeconds !== null &&
          Math.abs(m.positionSeconds - seconds) <= 1 / 48000 &&
          m
        );
      });
    } catch (error) {
      observation.failure = String(error);
      throw error;
    } finally {
      await save();
    }
  };
  const hold = async (seconds) => {
    const end = Date.now() + seconds * 1000;
    const began = performance.now();
    const rows = [];
    while (Date.now() < end) {
      const m = await media();
      assert.equal(m.error, null);
      assert.equal(m.session, session);
      rows.push({
        ...m,
        observationElapsedSeconds: (performance.now() - began) / 1000,
      });
      await delay(25);
    }
    return rows;
  };
  const capture = async (name, reference, kind, operation, extra = {}) => {
    const path = join(evidence, name + '.f32');
    const child = spawn(helper, [String(pid), '30', path], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let output = '';
    child.stdout.on('data', (bytes) => {
      output += bytes;
    });
    child.stderr.on('data', (bytes) => {
      output += bytes;
    });
    const ended = new Promise((done, reject) => {
      child.once('error', reject);
      child.once('exit', (code, signal) => done({ code, signal }));
    });
    void ended.catch(() => {});
    const row = {
      name,
      path,
      reference,
      kind,
      recorderPid: child.pid,
      ...extra,
    };
    report.captures.push(row);
    const failures = [];
    try {
      await until(
        'process capture READY',
        async () => {
          assert.equal(child.exitCode, null, output);
          assert.ok(
            output.length < 65536,
            'Capture output exceeded its diagnostic bound.',
          );
          return output.split('\n').some((line) => line.startsWith('READY '));
        },
        10000,
      );
      row.observations = await operation(row);
    } catch (error) {
      failures.push(error);
    } finally {
      if (child.exitCode === null && child.signalCode === null)
        child.kill('SIGTERM');
      try {
        row.exit = await Promise.race([
          ended,
          delay(5000, undefined, { ref: false }).then(() => {
            throw new Error('Capture cleanup timed out');
          }),
        ]);
      } catch (error) {
        if (child.exitCode === null && child.signalCode === null)
          child.kill('SIGKILL');
        await ended.catch(() => {});
        failures.push(error);
      } finally {
        if (failures.length)
          row.failures = failures.map((error) => error.stack ?? String(error));
        await writeFile(join(evidence, name + '.capture.log'), output);
        await save();
      }
    }
    if (failures.length)
      throw new AggregateError(failures, `Capture ${name} failed`);
    assert.deepEqual(row.exit, { code: 0, signal: null }, output);
    row.sha256 = hash(await readFile(path));
    row.metadataSha256 = hash(await readFile(path + '.json'));
    await save();
  };
  const loopCapture = (name) =>
    capture(
      name,
      source,
      'loop',
      async (captureRow) => {
        await act({ kind: 'pause', paused: false });
        const rows = await hold(2);
        captureRow.observations = rows;
        await act({ kind: 'pause', paused: true });
        const settled = rows.slice(10);
        assert.ok(settled.length > 10);
        assert.ok(
          settled.every(
            (m) =>
              m.positionSeconds >= 1.013 - 1 / 48000 &&
              m.positionSeconds <= 1.336 + 1 / 48000,
          ),
        );
        assert.ok(
          settled
            .slice(1)
            .filter(
              (m, i) => m.positionSeconds < settled[i].positionSeconds - 0.1,
            ).length >= 3,
        );
        return rows;
      },
      { loop: [1.013, 1.336] },
    );

  await menu('openSubtitles');
  const original = await until('native audio subtitle document', async () => {
    const d = await status();
    return d.loaded && d.cues[0]?.text === 'Keep this document' && d;
  });
  await menu('openMedia');
  let opened = await until('native audio ready', async () => {
    const m = await media();
    return m.loaded && m.durationSeconds === 4 && m.audioTrack !== null && m;
  });
  session = opened.session;
  assert.ok(opened.paused && opened.queuedAudioClock);
  const runtimeCommand = [
    resolve('scripts/record-native-runtime.py'),
    '--pid',
    String(pid),
    '--expect',
    binary,
    '--expect',
    resolve(process.env.VSF_EXPECT_MPV),
    '--output',
    join(evidence, 'native-audio-runtime.json'),
  ];
  const runtimeResult = await promisify(execFile)('python3', runtimeCommand, {
    timeout: 60000,
    maxBuffer: 1048576,
  });
  await writeFile(
    join(evidence, 'native-audio-runtime.log'),
    runtimeResult.stdout + runtimeResult.stderr,
  );
  report.runtimeCommand = runtimeCommand;
  report.runtimeReceiptSha256 = hash(
    await readFile(join(evidence, 'native-audio-runtime.json')),
  );
  await save();
  await act({ kind: 'loop', start: 1.013, end: 1.336 });
  await seek(1.013);
  await loopCapture('audio-loop-initial');

  await seek(2.5);
  await capture(
    'audio-past-b',
    source,
    'suffix',
    async (captureRow) => {
      await act({ kind: 'pause', paused: false });
      const rows = await hold(2);
      captureRow.observations = rows;
      assert.ok(
        rows.slice(10).every((m) => m.positionSeconds >= 2.5),
        'Past-B playback lost its clock or returned before the requested seek.',
      );
      assert.ok(
        rows.at(-1).paused && rows.at(-1).positionSeconds >= 4 - 1 / 48000,
        'Past-B playback did not reach paused EOF within its observation window.',
      );
      return rows;
    },
    { start: 2.5 },
  );
  await seek(1.013);
  await loopCapture('audio-loop-reentered');
  await seek(0.5);
  await act({ kind: 'pause', paused: false });
  const lead = await hold(1.5);
  report.observations.push({ name: 'before-a-lead-in', rows: lead });
  assert.ok(lead.some((m) => m.positionSeconds < 1.013));
  await loopCapture('audio-loop-after-lead-in');

  await act({ kind: 'pause', paused: false });
  const priorSession = session;
  await menu('openMedia');
  opened = await until('replacement paused without old session', async () => {
    const m = await media();
    return m.loaded && m.session > priorSession && m.durationSeconds === 4 && m;
  });
  session = opened.session;
  assert.ok(opened.paused);
  const refused = await browser.execute(async (old) => {
    try {
      await window.__TAURI__.core.invoke('media_action', {
        action: { kind: 'seek', seconds: 3 },
        session: old,
      });
      return null;
    } catch (error) {
      return String(error);
    }
  }, priorSession);
  assert.match(refused, /[Ss]tale|[Ss]uperseded/);
  assert.ok((await media()).paused);
  await seek(0);
  await capture(
    'audio-replacement',
    replacement,
    'suffix',
    async (captureRow) => {
      await act({ kind: 'pause', paused: false });
      const rows = await hold(4.5);
      captureRow.observations = rows;
      assert.ok(
        rows.some((m) => m.positionSeconds > 1.336 + 0.5),
        'Replacement retained old loop.',
      );
      assert.ok(
        rows.at(-1).paused && rows.at(-1).positionSeconds >= 4 - 1 / 48000,
      );
      return rows;
    },
    { start: 0 },
  );
  if (process.platform === 'linux') {
    await act({ kind: 'loop', start: 1.013, end: 1.336 });
    await seek(1.013);
    await act({ kind: 'pause', paused: false });
    const looping = await hold(1.5);
    await act({ kind: 'clearLoop' });
    const cleared = await hold(4.5);
    const settled = cleared.filter((m) => m.observationElapsedSeconds > 0.5);
    const live = settled.filter((m) => !m.paused);
    const missing = live.filter((m) => m.positionSeconds === null).length;
    const backward = Math.max(
      0,
      ...live
        .slice(1)
        .map((m, i) =>
          live[i].positionSeconds === null || m.positionSeconds === null
            ? 0
            : live[i].positionSeconds - m.positionSeconds,
        ),
    );
    report.observations.push({
      name: 'linux-loop-clear-clock',
      scope:
        'Sampled editor clock and EOF only; no clear-boundary PCM capture.',
      looping,
      cleared,
      maxBackwardSeconds: backward,
      missingClockSamples: missing,
      toleranceSeconds: 1 / 48000,
    });
    await save();
    assert.ok(
      live.length >= 10,
      'Clear did not retain enough live clock samples.',
    );
    assert.equal(missing, 0, 'Clear lost the live queued-audio clock.');
    assert.ok(
      live.every((m) => m.queuedAudioClock && !m.seeking),
      'Clear clock observations are not settled queued-audio observations.',
    );
    assert.ok(
      backward <= 1 / 48000,
      `Cleared editor clock moved backwards by ${backward} seconds.`,
    );
    assert.ok(
      cleared.at(-1).paused && cleared.at(-1).positionSeconds >= 4 - 1 / 48000,
      'Cleared editor playback did not finish at EOF.',
    );
  }
  await act({ kind: 'close' });
  await until('media fully closed', async () => !(await media()).loaded);
  const after = await status();
  assert.equal(after.revision, original.revision);
  assert.equal(after.cues[0].text, original.cues[0].text);
  assert.equal(after.dirty, false);
  for (const [path, sha256] of Object.entries(report.sources))
    assert.equal(hash(await readFile(path)), sha256);
  report.status = 'state-and-capture-complete-analysis-required';
  await save();
  const analysisCommand = [
    resolve('tests/native/audio-editor-content.py'),
    '--report',
    join(evidence, 'native-audio.json'),
    '--output',
    join(evidence, 'native-audio-content.json'),
  ];
  if (captureOnly) {
    await record({
      step: 'native audio state and captures complete; content analysis required',
      report: join(evidence, 'native-audio.json'),
      analysisCommand,
    });
    return;
  }
  try {
    const result = await promisify(execFile)(
      process.env.VSF_AUDIO_ANALYSIS_PYTHON,
      analysisCommand,
      { timeout: 180000, maxBuffer: 1048576 },
    );
    await writeFile(
      join(evidence, 'native-audio-analysis.log'),
      result.stdout + result.stderr,
    );
  } catch (error) {
    await writeFile(
      join(evidence, 'native-audio-analysis.log'),
      String(error.stdout ?? '') +
        String(error.stderr ?? '') +
        '\n' +
        String(error),
    );
    throw error;
  }
  await record({
    step: 'native audio re-entry/replacement state and captured content',
    report: join(evidence, 'native-audio.json'),
    content: join(evidence, 'native-audio-content.json'),
    analysisCommand,
  });
}
