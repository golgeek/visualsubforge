import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';

// Real component/IPC/file/CLI parity; no mocked quality reports or media needed.
export async function qualityChecking({
  browser,
  invoke,
  status,
  until,
  root,
  evidence,
  record,
  source,
  cli,
}) {
  const before = await status();
  const context = (await invoke('quality_settings')).current;
  const metrics = await browser.$(
    '//label[normalize-space(.)="Cue metrics"]/input',
  );
  await metrics.click();
  const accepted = await browser.$('[aria-label="Accepted cue metrics"]');
  await until('accepted cue metrics', () => accepted.isExisting());
  await (
    await browser.$('//button[starts-with(normalize-space(.), "Problems")]')
  ).click();
  const request = {
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
  const report = await until('real quality report', async () => {
    const value = await invoke('quality_read', { request });
    return value.phase === 'ready' && value;
  });
  const profilePath = join(root, 'active-quality.vsfpreset');
  await invoke('quality_profile_save', {
    path: profilePath,
    profile: context.profile,
  });
  const result = await new Promise((resolve, reject) => {
    execFile(
      cli,
      ['check', '--subtitles', source, '--profile', profilePath, '--json'],
      { timeout: 20_000, maxBuffer: 16 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error && ![1, 3].includes(error.code)) reject(error);
        else
          resolve({
            code: error?.code ?? 0,
            report: JSON.parse(stdout),
            stderr,
          });
      },
    );
  });
  assert.equal(result.code, 3); // The selected SW-derived profile declares partial coverage.
  assert.equal(result.report.profileHash, report.profileHash);
  assert.deepEqual(result.report.profile, report.profile);
  assert.deepEqual(
    result.report.metrics,
    report.metrics.map((row) => row.data),
  );
  assert.deepEqual(result.report.findings, report.findings);
  assert.equal(result.report.affectedCues, report.affectedCues);
  assert.equal(result.report.complete, false);
  assert.match(
    await accepted.getText(),
    new RegExp(
      `CPS ${report.selected.data.cps.value.toFixed(1).replace('.', '\\.')}`,
    ),
  );
  assert.deepEqual(await status(), before);
  await writeFile(
    join(evidence, 'quality-cli-parity.json'),
    JSON.stringify({ cli: result, desktop: report }, null, 2),
  );

  const editor = await browser.$('[aria-label="Subtitle text"]');
  const text = '{\\i1}Draft tags\\N';
  await editor.setValue(text);
  const draft = await browser.$(
    '[aria-label="Draft · not applied cue metrics"]',
  );
  await until('labeled draft metrics', () => draft.isExisting());
  assert.match(await draft.getText(), /12 UTF-16 units/);
  assert.deepEqual(await status(), before);
  const next = await browser.$('button=Next cue · F4');
  await until('problem navigation ready', () => next.isEnabled());
  await next.click();
  await until('protected problem navigation', () =>
    browser.$('button=Cancel navigation').then((b) => b.isExisting()),
  );
  await (await browser.$('button=Cancel navigation')).click();
  assert.equal(await editor.getValue(), text);
  assert.deepEqual(await status(), before);
  await next.click();
  await (await browser.$('button=Discard draft and navigate')).click();
  await until(
    'related problem cue selected',
    async () => (await editor.getValue()) === before.cues[1].text,
  );
  assert.deepEqual(await status(), before);
  await until(
    'settled selected metrics and findings',
    async () => (await next.isEnabled()) && (await accepted.isExisting()),
  );
  const layout = await browser.execute(() => ({
    listHeight: document.querySelector('.cue-list').getBoundingClientRect()
      .height,
    editorHeight: document
      .querySelector('[aria-label="Subtitle text"]')
      .getBoundingClientRect().height,
  }));
  assert.ok(layout.listHeight >= 64, 'Problems squeezed out the cue list');
  assert.ok(layout.editorHeight >= 34, 'Metrics squeezed out the text editor');
  await browser.execute(() => {
    document.querySelector('[aria-label="Subtitle Problems"]').scrollTop = 0;
  });
  await browser.saveScreenshot(join(evidence, 'quality-problems.png'));
  await browser.execute(() => {
    const panel = document.querySelector('[aria-label="Subtitle Problems"]');
    panel.scrollTop = panel.scrollHeight;
  });
  await browser.saveScreenshot(join(evidence, 'quality-findings.png'));
  await (await browser.$('button=Close Problems')).click();
  await metrics.click();
  await (await browser.$$('.cue-row'))[0].click();
  await until(
    'original cue selected',
    async () => (await editor.getValue()) === before.cues[0].text,
  );
  await record({
    step: 'real quality metrics/Problems/draft protection and executable CLI parity',
    findings: report.findingsCount,
    affectedCues: report.affectedCues,
    profileHash: report.profileHash,
    documentUnchanged: true,
    layout,
  });
}
