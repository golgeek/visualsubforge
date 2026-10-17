// Opt-in actual WebView/IPC measurement. Events are generated in the webview;
// rAF measures event-loop availability, not physical input or GPU presentation.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { createReadStream } from 'node:fs';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { release } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';

const interactionBudgetMs = 1000; // Task 011, declared before desktop measurements.
const time = (ms) => {
  const hours = Math.floor(ms / 3_600_000);
  const minutes = Math.floor(ms / 60_000) % 60;
  const seconds = Math.floor(ms / 1000) % 60;
  return `${[hours, minutes, seconds].map((v) => String(v).padStart(2, '0')).join(':')},${String(ms % 1000).padStart(3, '0')}`;
};
async function sha256(path) {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}
const percentile = (values, fraction) =>
  [...values].sort((a, b) => a - b)[Math.ceil(values.length * fraction) - 1];
async function rss(pid) {
  return new Promise((resolve, reject) => {
    execFile(
      'ps',
      ['-o', 'rss=', '-p', String(pid)],
      { timeout: 2000 },
      (error, out) => {
        if (error) reject(error);
        else {
          const bytes = Number(out.trim()) * 1024;
          if (!Number.isFinite(bytes) || bytes <= 0)
            reject(new Error('Invalid RSS sample'));
          else resolve(bytes);
        }
      },
    );
  });
}

export async function qualityPerformance({
  browser,
  menu,
  invoke,
  status,
  until,
  root,
  evidence,
  record,
  binary,
  pid,
}) {
  const text =
    '<i>This is a representative subtitle line.</i>\nAnother line with punctuation and Unicode: 日本語 😀.';
  const source = join(root, 'quality-10000.srt');
  const bytes = Array.from(
    { length: 10_000 },
    (_, i) =>
      `${i + 1}\n${time(i * 4100)} --> ${time(i * 4100 + 4000)}\n${text}\n\n`,
  ).join('');
  await writeFile(source, bytes, { flag: 'wx' });
  const result = {
    schema: 1,
    date: new Date().toISOString(),
    platform: process.platform,
    architecture: process.arch,
    osRelease: release(),
    binary,
    binarySha256: await sha256(binary),
    sourceSha256: await sha256(source),
    sourceBytes: Buffer.byteLength(bytes),
    cueTextBytes: Buffer.byteLength(text) * 10_000,
    cues: 10_000,
    build: 'debug desktop-e2e',
    interactionBudgetMs,
    pageLimit: 200,
    scope:
      'Actual Tauri/WebView/IPC with generated DOM events; no native input, GPU, or audio claim. RSS is the application process only, not WebKit subprocesses. Desktop round trips are separate from the release pure-engine budget.',
    runs: [],
    rssSamples: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'quality-performance.json'),
      `${JSON.stringify(result, null, 2)}\n`,
    );
  await save();
  await menu('openSubtitles');
  const before = await until('10,000-cue document', async () => {
    const doc = await status();
    return doc.cues.length === 10_000 && doc;
  });
  await until('large-document workspace ready', async () =>
    (await browser.$('main'))
      .getAttribute('aria-busy')
      .then((v) => v === 'false'),
  );
  result.baselineRssBytes = await rss(pid);
  let sampling = true;
  const sampler = (async () => {
    while (sampling) {
      try {
        result.rssSamples.push({ at: Date.now(), bytes: await rss(pid) });
      } catch (error) {
        result.rssError = String(error);
        break;
      }
      await delay(50);
    }
  })();
  try {
    const request = {
      session: 0,
      generation: before.generation,
      revision: before.revision,
      run: true,
      restart: false,
      offset: 0,
      limit: 200,
      metricsOffset: 0,
      metricsLimit: 200,
      selected: before.cues[0].id,
      rule: null,
      severity: null,
      draft: null,
    };
    await browser.execute((request) => {
      const bench = {
        request,
        frames: [],
        active: true,
        last: performance.now(),
        read: (overrides = {}) =>
          window.__TAURI__.core.invoke('quality_read', {
            request: { ...request, ...overrides },
          }),
        button: (label) =>
          [...document.querySelectorAll('button')].find(
            (b) => b.textContent.trim() === label,
          ),
        wait: async (probe, label, timeout = 3000) => {
          const deadline = performance.now() + timeout;
          while (performance.now() < deadline) {
            const value = await probe();
            if (value) return value;
            await new Promise((done) => setTimeout(done, 10));
          }
          throw new Error(`Timed out: ${label}`);
        },
        frame: () => new Promise((done) => requestAnimationFrame(() => done())),
      };
      window.__qualityBench = bench;
      const tick = (now) => {
        if (!bench.active) return;
        bench.frames.push(now - bench.last);
        bench.last = now;
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    }, request);
    await (
      await browser.$('//label[normalize-space(.)="Cue metrics"]/input')
    ).click();
    await (
      await browser.$('//button[starts-with(normalize-space(.), "Problems")]')
    ).click();
    await until('large-document metrics and findings', async () =>
      browser.execute(
        () =>
          !!document.querySelector('[aria-label="Accepted cue metrics"]') &&
          document.querySelectorAll('.finding').length === 100,
      ),
    );
    const warm = await invoke('quality_read', { request });
    assert.equal(warm.phase, 'ready');
    result.profile = warm.profile;
    result.profileHash = warm.profileHash;
    result.findingsCount = warm.findingsCount;
    result.affectedCues = warm.affectedCues;
    result.dom = await browser.execute(() => ({
      cueRows: document.querySelectorAll('.cue-row').length,
      findings: document.querySelectorAll('.finding').length,
      editorBottom: document.querySelector('.editor').getBoundingClientRect()
        .bottom,
      historyBottom: document
        .querySelector('.editorfooter')
        .getBoundingClientRect().bottom,
      shortcutsTop: document
        .querySelector('.shortcut-help')
        .getBoundingClientRect().top,
    }));
    await save();
    assert.ok(
      result.dom.historyBottom <= result.dom.editorBottom,
      'The editor clips its history buttons',
    );
    assert.ok(
      result.dom.editorBottom <= result.dom.shortcutsTop,
      'Recovery and metrics make the workspace overlap the shortcut panel',
    );
    assert.ok(result.dom.cueRows > 0 && result.dom.cueRows <= 200);
    assert.equal(result.dom.findings, 100);
    assert.equal(warm.metrics.length, 200);
    assert.equal(warm.findings.length, 200);
    for (let i = 0; i < 20; i++) {
      const run = await browser.execute(async () => {
        const b = window.__qualityBench;
        const began = performance.now();
        const first = await b.read({ restart: true });
        const report = await b.wait(async () => {
          const view = await b.read();
          if (view.phase === 'failed') throw new Error(view.error);
          return view.phase === 'ready' && view;
        }, 'completed large check');
        return {
          elapsedMs: performance.now() - began,
          admissionPhase: first.phase,
          findings: report.findings.length,
          metrics: report.metrics.length,
          pageBytes: new TextEncoder().encode(JSON.stringify(report)).length,
        };
      });
      result.runs.push(run);
      await save();
      assert.equal(run.admissionPhase, 'checking');
      assert.equal(run.findings, 200);
      assert.equal(run.metrics, 200);
    }
    result.checkRoundTripP95Ms = percentile(
      result.runs.map((r) => r.elapsedMs),
      0.95,
    );
    result.pageBoundRejection = await browser.execute(async () => {
      try {
        await window.__qualityBench.read({ limit: 201 });
        return null;
      } catch (error) {
        return String(error);
      }
    });
    assert.match(result.pageBoundRejection, /200/);
    result.typing = await browser.execute(async () => {
      const b = window.__qualityBench;
      const first = await b.read({ restart: true });
      const began = performance.now();
      const editor = document.querySelector('[aria-label="Subtitle text"]');
      editor.focus();
      editor.value = 'Large-document draft, still unaccepted';
      editor.dispatchEvent(new Event('input', { bubbles: true }));
      await b.frame();
      return {
        elapsedMs: performance.now() - began,
        admissionPhase: first.phase,
        draft: editor.value,
        applyEnabled: !b.button('Apply edit').disabled,
      };
    });
    await save();
    assert.equal(result.typing.admissionPhase, 'checking');
    assert.equal(result.typing.applyEnabled, true);
    assert.ok(result.typing.elapsedMs < interactionBudgetMs);
    assert.deepEqual(await status(), before);
    await (await browser.$('button=Revert draft')).click();
    await until(
      'large draft reverted',
      async () => !(await (await browser.$('button=Apply edit')).isEnabled()),
    );
    result.cancellation = await browser.execute(async () => {
      const b = window.__qualityBench;
      await b.wait(async () => {
        const view = await b.read({ draft: { kind: 'clear' } });
        return view.phase === 'ready' && !view.draftChecking;
      }, 'draft worker drained');
      const first = await b.read({ restart: true });
      const began = performance.now();
      b.button('Pause checking').click();
      await b.frame();
      const visibleMs = performance.now() - began;
      const paused = document.querySelector(
        '[aria-label="Subtitle Problems"] header [role="status"]',
      ).textContent;
      const view = await b.wait(async () => {
        const v = await b.read({ run: false });
        return v.phase === 'cancelled' && v;
      }, 'cancelled checker');
      return {
        elapsedMs: performance.now() - began,
        visibleMs,
        paused,
        admissionPhase: first.phase,
        phase: view.phase,
      };
    });
    await save();
    assert.equal(result.cancellation.admissionPhase, 'checking');
    assert.equal(result.cancellation.paused, 'Checking paused');
    assert.ok(result.cancellation.visibleMs < interactionBudgetMs);
    assert.ok(result.cancellation.elapsedMs < interactionBudgetMs);
    assert.deepEqual(await status(), before);
    result.invalidation = await browser.execute(async () => {
      const b = window.__qualityBench;
      const editor = document.querySelector('[aria-label="Subtitle text"]');
      editor.value = 'Accepted large-document edit';
      editor.dispatchEvent(new Event('input', { bubbles: true }));
      await b.frame();
      const began = performance.now();
      b.button('Apply edit').click();
      const rejection = await b.wait(async () => {
        try {
          await b.read({ run: false });
          return null;
        } catch (error) {
          return String(error);
        }
      }, 'stale revision rejected');
      await b.wait(
        () =>
          document.querySelector('main').getAttribute('aria-busy') ===
            'false' &&
          document
            .querySelector('.cue-row')
            ?.textContent.includes('Accepted large-document edit'),
        'accepted edit reconciled into the cue list',
      );
      await b.wait(
        () => !document.querySelector('[aria-label="Accepted cue metrics"]'),
        'obsolete metrics removed',
      );
      // WebDriver reserves a top-level `error` key in returned objects.
      return { elapsedMs: performance.now() - began, rejection };
    });
    await save();
    assert.match(result.invalidation.rejection, /Subtitles changed/);
    assert.ok(result.invalidation.elapsedMs < interactionBudgetMs);
    const edited = await status();
    assert.equal(edited.revision, before.revision + 1);
    assert.equal(edited.cues[0].text, 'Accepted large-document edit');
    await (await browser.$('button=Undo edit')).click();
    const restored = await until('large edit Undo', async () => {
      const doc = await status();
      return doc.revision === before.revision + 2 && doc;
    });
    assert.deepEqual(restored.cues, before.cues);
    assert.equal(restored.dirty, before.dirty);
    assert.equal(restored.requiresSaveAs, before.requiresSaveAs);
    // Exercise the real modal with 9,999 candidates, never sending the full
    // before/after reports to the frontend and never accepting this trial.
    await (await browser.$('button=Enforce minimum gaps…')).click();
    const dialog = await browser.$('dialog[aria-labelledby="gap-title"]');
    await until('large gap modal', () => dialog.isDisplayed());
    const previewBegan = performance.now();
    await (await dialog.$('button=Create gap preview')).click();
    await until('large gap review', async () =>
      (await dialog.getText()).includes('9999 safe changes'),
    );
    result.gapPreviewMs = performance.now() - previewBegan;
    result.gapRows = await browser.execute(
      () => document.querySelectorAll('dialog tbody tr').length,
    );
    assert.equal(result.gapRows, 100);
    await browser.saveScreenshot(join(evidence, 'quality-10000-gap.png'));
    await (await dialog.$('button=Close gap review')).click();
    await until(
      'large gap modal closed',
      async () => !(await dialog.isExisting()),
    );
    assert.deepEqual(await status(), restored);
    await until('large workspace restored', () =>
      browser.$('button=Enforce minimum gaps…').then((b) => b.isEnabled()),
    );
    await (await browser.$('button=Check again')).click();
    await until('large findings restored', async () =>
      browser.execute(
        () => document.querySelectorAll('.finding').length === 100,
      ),
    );
    await browser.saveScreenshot(join(evidence, 'quality-10000-problems.png'));
    result.status = 'passed';
    await record({
      step: '10,000-cue actual desktop responsiveness',
      evidence: 'quality-performance.json',
      checkRoundTripP95Ms: result.checkRoundTripP95Ms,
      typingMs: result.typing.elapsedMs,
      cancellationMs: result.cancellation.elapsedMs,
      invalidationMs: result.invalidation.elapsedMs,
    });
  } catch (error) {
    result.status = 'failed';
    result.error = String(error.stack ?? error);
    throw error;
  } finally {
    sampling = false;
    await sampler;
    result.framesMs = await browser
      .execute(() => {
        const b = window.__qualityBench;
        if (!b) return [];
        b.active = false;
        return b.frames;
      })
      .catch(() => []);
    result.frameGapP95Ms = percentile(result.framesMs, 0.95);
    result.frameGapMaxMs = Math.max(0, ...result.framesMs);
    result.peakRssBytes = Math.max(
      result.baselineRssBytes,
      ...result.rssSamples.map((s) => s.bytes),
    );
    result.additionalRssBytes = result.peakRssBytes - result.baselineRssBytes;
    await save();
  }
}
