// Real Settings UI, generated IPC and isolated filesystem. This does not replace
// the controlled-response browser tests or native media/GPU validation.
import assert from 'node:assert/strict';
import { mkdir, readFile, rmdir } from 'node:fs/promises';
import { join } from 'node:path';

export async function qualitySettings({
  browser,
  menu,
  invoke,
  status,
  until,
  root,
  evidence,
  record,
}) {
  const before = await status();
  const context = (await invoke('quality_settings')).current;
  await menu('settings');
  const apply = await browser.$('button=Apply settings');
  await until('loaded quality Settings', () => apply.isEnabled());
  const select = await browser.$('[aria-label="Quality profile"]');
  async function chooseProfile(id) {
    // The pinned embedded driver implements option clicks as HTMLElement.click,
    // which does not select an option in WKWebView. Dispatch the control's real
    // DOM events explicitly; Rust IPC and persistence remain unmodified.
    await browser.execute((value) => {
      const control = document.querySelector('[aria-label="Quality profile"]');
      control.value = value;
      control.dispatchEvent(new Event('input', { bubbles: true }));
      control.dispatchEvent(new Event('change', { bubbles: true }));
    }, id);
    await until(
      'quality profile selection',
      async () => (await select.getValue()) === id,
    );
  }
  await chooseProfile('sw3-uk-vo-subset');
  assert.match(
    await (await browser.$('.coverage')).getText(),
    /Partial coverage/,
  );
  assert.equal((await invoke('quality_settings')).current.epoch, context.epoch);
  assert.deepEqual(await status(), before);
  const importButton = await browser.$('button=Import quality profile');
  await importButton.click(); // cancelled file selection
  await until('cancelled profile import', () => importButton.isEnabled());
  assert.equal(await select.getValue(), 'sw3-uk-vo-subset');
  await importButton.click(); // malformed profile file
  const error = await browser.$('dialog[open] [role="alert"]');
  await until('invalid profile import', async () =>
    /Invalid|invalid/.test(await error.getText().catch(() => '')),
  );
  assert.equal(await select.getValue(), 'sw3-uk-vo-subset');
  await (await browser.$('button=Export quality profile')).click();
  const path = join(root, 'exported.vsfpreset');
  const exported = await until(
    'exported complete quality profile',
    async () => {
      try {
        return JSON.parse(await readFile(path, 'utf8'));
      } catch {
        return null;
      }
    },
  );
  assert.equal(exported.id, 'sw3-uk-vo-subset');
  assert.equal(exported.rules.speed.maximumCps, null);
  assert.ok(exported.limitations.length > 0);
  await chooseProfile('vsf-default');
  await importButton.click();
  await until(
    'imported profile draft',
    async () => (await select.getValue()) === exported.id,
  );
  await browser.saveScreenshot(join(evidence, 'quality-settings.png'));
  await (await browser.$('button=Customize a copy')).click();
  const name = await browser.$(
    '//label[contains(., "Custom profile name")]/input',
  );
  await name.setValue('Desktop quality');
  const maximum = await browser.$(
    '//label[contains(., "Maximum duration")]/input',
  );
  await maximum.setValue('6000');
  await (await browser.$('button=Import timing preset')).click();
  const gap = await browser.$('//label[contains(., "Between cues")]/input');
  await until(
    'timing-only import',
    async () => (await gap.getValue()) === '167',
  );
  assert.equal(await maximum.getValue(), '6000');

  // A real atomic replacement failure, inside this test's isolated app-data
  // directory. It must not accept the draft or discard the previous policy.
  const settings = join(root, 'settings-v1.json');
  await mkdir(settings);
  await apply.click();
  await until(
    'failed Settings write',
    async () => (await error.getText().catch(() => '')).length > 0,
  );
  assert.deepEqual((await invoke('quality_settings')).current, context);
  assert.deepEqual(await status(), before);
  await rmdir(settings);
  await apply.click();
  await until(
    'closed applied Settings',
    async () => !(await (await browser.$('dialog[open]')).isExisting()),
  );
  const stored = JSON.parse(await readFile(settings, 'utf8'));
  assert.ok(stored.quality.id.startsWith('custom-'));
  assert.equal(stored.quality.rules.duration.maximumMs, 6000);
  assert.deepEqual(stored.quality.rules.speed, exported.rules.speed);
  assert.equal(stored.quality.timing.cueGapMs, 167);
  assert.equal(stored.timing.cueGapMs, 167);
  assert.deepEqual(
    (await invoke('quality_settings')).current.profile,
    stored.quality,
  );
  assert.deepEqual(await status(), before);

  await menu('settings');
  await until('reopened Settings', async () =>
    (await browser.$('button=Apply settings')).isEnabled(),
  );
  const current = (await invoke('quality_settings')).current;
  // Advance the real accepted epoch while the real dialog retains its draft.
  await invoke('preferences_save', {
    preferences: stored,
    epoch: current.epoch,
  });
  await (await browser.$('button=Apply settings')).click();
  await until('stale Settings rejection', async () =>
    /Workspace settings changed/.test(
      await (
        await browser.$('dialog[open] [role="alert"]')
      )
        .getText()
        .catch(() => ''),
    ),
  );
  assert.deepEqual(JSON.parse(await readFile(settings, 'utf8')), stored);
  await (await (await browser.$('dialog[open]')).$('button=Cancel')).click();
  await record({
    step: 'real quality selection/customization/import/export, failed write, retry and stale Apply',
    profileId: stored.quality.id,
    documentUnchanged: true,
  });
  return stored.quality;
}
