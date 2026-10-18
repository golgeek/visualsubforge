// Task021: the same six 10k workloads as language-benchmark.rs. One-second
// interaction/cancellation/paging/acceptance gates are unchanged. Whole worker
// completion is recorded separately; production retains its ten-second limit.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
function time(ms, extension) {
  const h = Math.floor(ms / 3600000),
    m = Math.floor(ms / 60000) % 60,
    s = Math.floor(ms / 1000) % 60;
  return extension === 'srt'
    ? `${[h, m, s].map((v) => String(v).padStart(2, '0')).join(':')},${String(ms % 1000).padStart(3, '0')}`
    : `${h}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}.${String(Math.floor((ms % 1000) / 10)).padStart(2, '0')}`;
}
async function fixture(language, extension) {
  let source =
    extension === 'srt'
      ? ''
      : (
          await readFile(
            resolve(`tests/fixtures/subtitles/styles.${extension}`),
            'utf8',
          )
        ).split('Dialogue:')[0];
  for (let i = 0; i < 10000; i++) {
    const base =
      language === 'fr'
        ? "bonjor, les e\u0301le\u0300ves aiment l'amour ! 😀"
        : "helo, the students don't know... 😀";
    let text = `${base} Fanword${i}`;
    if (i % 10 < 2) text += ` Fanword${10000 + i}`;
    if (i % 10 === 1)
      text = extension === 'srt' ? `<i>${text}</i>` : `{\\i1}${text}{\\i0}`;
    if (i % 10 === 2) text = `{\\an8}${text}\\N—hello`;
    if (i % 10 === 3) text = `—${text}`;
    if (i % 10 === 8) text = `{\\k20}${text}`;
    if (i % 10 === 9) text = '{\\p1}m 0 0 l 5 5';
    const start = time(i * 5100, extension),
      end = time(i * 5100 + 5000, extension);
    source +=
      extension === 'srt'
        ? `${i + 1}\n${start} --> ${end}\n${text}\n\n`
        : extension === 'ass'
          ? `Dialogue: Default,${end},${start},0,,0000,0000,0000,,retain,${text}\n`
          : `Dialogue: Marked=0,${start},${end},Default,,0000,0000,0000,,${text}\n`;
  }
  return source;
}

export async function languagePerformance({
  restart,
  status,
  invoke,
  menu,
  until,
  evidence,
  root,
  binary,
  record,
}) {
  const report = {
    status: 'running',
    budgetMs: 1000,
    cues: 10000,
    distinctUnknownWords: 10000,
    binary,
    binarySha256: sha256(await readFile(binary)),
    scope:
      'Actual native webview/Rust IPC; generated DOM events, no physical input or GPU/audio acceptance. Whole background preparation is recorded separately from interaction gates.',
    cases: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'language-performance.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  await save();
  try {
    for (const extension of ['srt', 'ass', 'ssa'])
      for (const language of ['en-US', 'fr']) {
        const bytes = await fixture(language, extension),
          path = join(root, `language-10000-${language}.${extension}`);
        await writeFile(path, bytes);
        const browser = await restart([{ kind: 'subtitles', path }]);
        await menu('openSubtitles');
        const baseline = await until('10k language source loaded', async () => {
          const d = await status();
          return d.cues.length === 10000 && d;
        });
        await until('10k language workspace', () =>
          browser.execute(
            () =>
              document.querySelector('main')?.getAttribute('aria-busy') ===
              'false',
          ),
        );
        const preferences = await invoke('preferences_get'),
          context = (await invoke('quality_settings')).current;
        preferences.quality = {
          ...preferences.quality,
          id: `language-scale-${language}`,
          name: `Language scale ${language}`,
          limitations: [],
          languageTools: {
            dictionary: language,
            spelling: true,
            typography: true,
            capitalization: true,
            dialogueMarkers: true,
          },
        };
        preferences.quality.timing.sceneBeforeMs =
          preferences.quality.timing.sceneAfterMs = 0;
        preferences.timing = {
          ...preferences.timing,
          ...preferences.quality.timing,
        };
        preferences.personalWords = { englishUs: [], french: [] };
        await invoke('preferences_save', { preferences, epoch: context.epoch });
        await menu('settings');
        await until('scale Settings loaded', () =>
          browser.$('button=Apply settings').then((b) => b.isEnabled()),
        );
        await (await browser.$('button=Apply settings')).click();
        await until('scale Settings applied', () =>
          browser.execute(
            () =>
              !document.querySelector('dialog[open]') &&
              document.querySelector('main')?.getAttribute('aria-busy') ===
                'false',
          ),
        );
        const result = {
          extension,
          language,
          sourceSha256: sha256(bytes),
          sourceBytes: Buffer.byteLength(bytes),
          timings: {},
        };
        report.cases.push(result);
        await save();
        await browser.execute(
          (doc) => {
            const invoke = window.__TAURI__.core.invoke;
            const b = {
              invoke,
              doc,
              state: () =>
                invoke('document_action', {
                  action: { kind: 'status' },
                  generation: 0,
                  revision: 0,
                }),
              ready: () =>
                document.querySelector('main').getAttribute('aria-busy') ===
                'false',
              button: (label) =>
                [...document.querySelectorAll('button')].find(
                  (e) => e.textContent.trim() === label,
                ),
              input: (value) => {
                const e = document.querySelector(
                  '[aria-label="Subtitle text"]',
                );
                e.value = value;
                e.dispatchEvent(new Event('input', { bubbles: true }));
              },
              frame: () => new Promise((r) => requestAnimationFrame(r)),
              wait: async (probe) => {
                const end = performance.now() + 20000;
                while (performance.now() < end) {
                  const v = await probe();
                  if (v) return v;
                  await new Promise((r) => setTimeout(r, 10));
                }
                throw new Error('Language desktop operation did not settle');
              },
              read: (overrides = {}) =>
                invoke('quality_read', {
                  request: {
                    session: 0,
                    generation: doc.generation,
                    revision: doc.revision,
                    run: true,
                    restart: false,
                    offset: 0,
                    limit: 200,
                    metricsOffset: 0,
                    metricsLimit: 0,
                    selected: null,
                    rule: 'spelling',
                    severity: null,
                    draft: null,
                    ...overrides,
                  },
                }),
              view: (id) => invoke('text_view', { id, offset: 0, limit: 200 }),
            };
            window.__languageBench = b;
          },
          { generation: baseline.generation, revision: baseline.revision },
        );
        const checked = await browser.execute(async () => {
          const b = window.__languageBench;
          const problems = [...document.querySelectorAll('button')].find((e) =>
            e.textContent.trim().startsWith('Problems'),
          );
          if (problems.getAttribute('aria-expanded') !== 'true')
            problems.click();
          await b.wait(
            () => document.querySelectorAll('.finding').length === 100,
          );
          const start = performance.now();
          await b.read({ restart: true });
          const view = await b.wait(async () => {
            const v = await b.read();
            if (v.phase === 'failed') throw new Error(v.error);
            return v.phase === 'ready' && v;
          });
          const analysisMs = performance.now() - start,
            pages = [],
            unique = new Set();
          let target;
          for (let offset = 0; offset < view.filteredCount; offset += 200) {
            const began = performance.now(),
              page = await b.read({ offset });
            pages.push(performance.now() - began);
            for (const f of page.findings) {
              if (f.language?.word.startsWith('Fanword'))
                unique.add(f.language.word);
              if (f.cueIndex === 0 && f.language?.start === 0)
                target = f.language;
            }
          }
          b.target = target;
          return {
            analysisMs,
            pageP95Ms: pages.sort((a, b) => a - b)[
              Math.ceil(pages.length * 0.95) - 1
            ],
            distinctUnknownWords: unique.size,
            coverage: view.languageCoverage,
            target,
            visibleFindings: document.querySelectorAll('.finding').length,
            visibleCues: document.querySelectorAll('.cue-row').length,
          };
        });
        assert.equal(checked.distinctUnknownWords, 10000);
        assert.equal(checked.coverage.skippedCues, 2000);
        assert.ok(checked.target);
        assert.ok(checked.visibleFindings <= 100 && checked.visibleCues <= 200);
        result.analysisMs = checked.analysisMs;
        result.coverage = checked.coverage;
        result.timings.pageP95Ms = checked.pageP95Ms;
        await save();
        const asyncWork = await browser.execute(
          async (replacement) => {
            const b = window.__languageBench;
            const first = await b.read({ restart: true });
            const start = performance.now(),
              original = document.querySelector(
                '[aria-label="Subtitle text"]',
              ).value;
            b.input(`${original}!`);
            await b.frame();
            const typingMs = performance.now() - start;
            const cancelStart = performance.now();
            b.button('Pause checking').click();
            await b.invoke('quality_cancel', { id: first.id });
            await b.wait(
              async () => (await b.read({ run: false })).phase === 'cancelled',
            );
            const checkingCancellationMs = performance.now() - cancelStart;
            b.button('Revert draft').click();
            await b.frame();
            const operation = {
              kind: 'languageCorrection',
              cueIndex: 0,
              start: b.target.start,
              end: b.target.end,
              original: b.target.word,
              replacement,
            };
            b.operation = operation;
            const id = await b.invoke('text_start', { ...b.doc, operation });
            const textCancelStart = performance.now();
            await b.invoke('text_cancel', { id });
            await b.wait(async () => (await b.view(id)).phase === 'cancelled');
            return {
              typingMs,
              checkingCancellationMs,
              textCancellationMs: performance.now() - textCancelStart,
              admissionPhase: first.phase,
            };
          },
          language === 'fr' ? 'bonjour' : 'hello',
        );
        assert.equal(asyncWork.admissionPhase, 'checking');
        for (const key of [
          'typingMs',
          'checkingCancellationMs',
          'textCancellationMs',
        ])
          result.timings[key] = asyncWork[key];
        assert.deepEqual(await status(), baseline);
        await save();
        // Drive the visible correction dialog with the same exact finding, then
        // measure actual acceptance reconciliation and Undo in the native webview.
        const reviewed = await browser.execute(async () => {
          const b = window.__languageBench;
          b.button('Check again').click();
          await b.frame();
          await b.wait(
            () =>
              document.querySelector(
                '[aria-label="Subtitle Problems"] .findings[aria-busy="false"] .finding button:not(:disabled)',
              ) && document.querySelector('[aria-label="Problem rule"]'),
          );
          const select = document.querySelector('[aria-label="Problem rule"]');
          select.value = 'spelling';
          select.dispatchEvent(new Event('change', { bubbles: true }));
          await b.frame();
          await b.wait(
            () => b.button('Suggestions') && !b.button('Suggestions').disabled,
          );
          b.button('Suggestions').click();
          await b.wait(() => b.button('Get spelling suggestions'));
          b.button('Get spelling suggestions').click();
          await b.wait(() => b.button(`Preview “${b.operation.replacement}”`));
          const began = performance.now();
          b.button(`Preview “${b.operation.replacement}”`).click();
          await b.wait(
            () =>
              b.button('Accept text repairs') &&
              !b.button('Accept text repairs').disabled,
          );
          const previewMs = performance.now() - began;
          const pre = await b.state();
          if (pre.revision !== b.doc.revision)
            throw new Error('Preview modified source');
          const acceptStart = performance.now();
          b.button('Accept text repairs').click();
          await b.wait(
            async () =>
              (await b.state()).revision === pre.revision + 1 &&
              b.ready() &&
              !document.querySelector('dialog[open]'),
          );
          await b.frame();
          const acceptanceMs = performance.now() - acceptStart,
            accepted = await b.state();
          if (
            accepted.cues[0].text !==
            pre.cues[0].text.replace(
              b.operation.original,
              b.operation.replacement,
            )
          )
            throw new Error('Wrong selected correction');
          const undoStart = performance.now();
          b.button('Undo edit').click();
          await b.wait(
            async () =>
              (await b.state()).revision === pre.revision + 2 && b.ready(),
          );
          await b.frame();
          b.doc.revision = pre.revision + 2;
          return {
            previewMs,
            acceptanceMs,
            undoMs: performance.now() - undoStart,
          };
        });
        result.previewMs = reviewed.previewMs;
        result.timings.acceptanceMs = reviewed.acceptanceMs;
        result.timings.undoMs = reviewed.undoMs;
        assert.deepEqual((await status()).cues, baseline.cues);
        await save();
        result.timings.invalidationMs = await browser.execute(async () => {
          const b = window.__languageBench;
          const id = await b.invoke('text_start', {
            ...b.doc,
            operation: b.operation,
          });
          await b.wait(async () => {
            const v = await b.view(id);
            if (v.phase === 'failed') throw new Error(v.error);
            return v.phase === 'reviewing';
          });
          const original = document.querySelector(
            '[aria-label="Subtitle text"]',
          ).value;
          b.input(`${original}!`);
          await b.frame();
          const start = performance.now();
          b.button('Apply edit').click();
          await b.wait(async () => {
            const v = await b.view(id);
            return v.phase === 'stale' && !v.rows.length && b.ready();
          });
          const elapsed = performance.now() - start;
          b.button('Undo edit').click();
          await b.wait(
            async () =>
              (await b.state()).revision === b.doc.revision + 2 && b.ready(),
          );
          return elapsed;
        });
        assert.deepEqual((await status()).cues, baseline.cues);
        assert.equal((await status()).dirty, false);
        assert.equal(await readFile(path, 'utf8'), bytes);
        await browser.saveScreenshot(
          join(evidence, `language-10000-${extension}-${language}.png`),
        );
        await save();
        for (const [name, ms] of Object.entries(result.timings))
          assert.ok(
            ms < report.budgetMs,
            `${extension}/${language} ${name}: ${ms} ms`,
          );
        result.status = 'passed';
        await record({
          step: `10k native language ${extension}/${language}`,
          ...result,
        });
        await save();
      }
    report.status = 'passed';
  } catch (error) {
    report.status = 'failed';
    report.error = `${error.message ?? error}\n${error.stack ?? ''}`;
    throw error;
  } finally {
    await save();
  }
}
