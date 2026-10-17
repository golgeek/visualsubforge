import assert from 'node:assert/strict';
import { join } from 'node:path';

export async function gapReview({ browser, status, until, evidence, record }) {
  const before = await status();
  const text = await browser.$('[aria-label="Subtitle text"]');
  await text.setValue('Unapplied gap review draft');
  await (await browser.$('button=Enforce minimum gaps…')).click();
  let dialog = await browser.$('dialog[aria-labelledby="gap-title"]');
  await until('gap review modal', () => dialog.isDisplayed());
  assert.equal(
    await (await dialog.$('button=Create gap preview')).isEnabled(),
    false,
  );
  await (await dialog.$('button=Cancel gap review')).click();
  await until(
    'gap modal cancellation',
    async () => !(await dialog.isExisting()),
  );
  assert.equal(await text.getValue(), 'Unapplied gap review draft');
  assert.deepEqual(await status(), before);
  await until('gap close restored workspace interaction', () =>
    browser
      .$('button=Enforce minimum gaps…')
      .then((button) => button.isEnabled()),
  );
  await (await browser.$('button=Enforce minimum gaps…')).click();
  // The first modal was unmounted; the embedded driver retains its element ID.
  dialog = await browser.$('dialog[aria-labelledby="gap-title"]');
  await until('reopened gap review', () => dialog.isDisplayed());
  await (await dialog.$('button=Discard cue draft')).click();
  await (await dialog.$('button=Create gap preview')).click();
  await until('completed gap proposal', async () =>
    (await dialog.getText()).includes('1 safe changes'),
  );
  assert.deepEqual(await status(), before);
  const accept = await dialog.$('button=Accept gap changes');
  assert.equal(await accept.isEnabled(), false);
  await (await dialog.$('input[type="checkbox"]')).click();
  await until('explicit partial-plan acknowledgment', () => accept.isEnabled());
  await browser.saveScreenshot(join(evidence, 'gap-preview.png'));
  await accept.click();
  await until(
    'gap proposal accepted',
    async () => !(await dialog.isExisting()),
  );
  // Unmount precedes the asynchronous native occlusion/focus restoration.
  // The workspace's modal guard stays active until that transition completes.
  await until('accepted gap dialog restored workspace interaction', () =>
    browser
      .$('button=Enforce minimum gaps…')
      .then((button) => button.isEnabled()),
  );
  const changed = await status();
  assert.equal(changed.revision, before.revision + 1);
  assert.equal(changed.cues[0].endMs, 1953); // 2120 ms following start minus accepted 167 ms gap.
  assert.equal(changed.cues[1].endMs, before.cues[1].endMs);
  assert.equal(changed.requiresSaveAs, before.requiresSaveAs);
  assert.equal(changed.dirty, true);
  for (let i = 0; i < before.cues.length; i++) {
    assert.equal(changed.cues[i].text, before.cues[i].text);
    assert.equal(changed.cues[i].startMs, before.cues[i].startMs);
    assert.equal(changed.cues[i].id, before.cues[i].id);
  }
  await (await browser.$('button=Undo edit')).click();
  await until(
    'one gap Undo',
    async () => (await status()).revision === before.revision + 2,
  );
  const undone = await status();
  assert.deepEqual(undone.cues, before.cues);
  assert.equal(undone.dirty, before.dirty);
  assert.equal(undone.canUndo, before.canUndo);
  await (await browser.$('button=Redo edit')).click();
  await until(
    'one gap Redo',
    async () => (await status()).revision === before.revision + 3,
  );
  assert.deepEqual((await status()).cues, changed.cues);
  await (await browser.$('button=Undo edit')).click();
  await until(
    'gap fixture restored',
    async () => (await status()).revision === before.revision + 4,
  );
  const restored = await status();
  assert.deepEqual(restored.cues, before.cues);
  await record({
    step: 'real gap preview, explicit no-media acknowledgment, atomic acceptance and Undo/Redo',
    changed: 1,
    originalRevision: before.revision,
    acceptedRevision: changed.revision,
    sourcePreserved: true,
  });
  return restored;
}
