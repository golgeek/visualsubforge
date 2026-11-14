import { discardCue } from './editor-input.mjs';
import { chooseCommand } from './commands.mjs';
import assert from 'node:assert/strict';
import { join } from 'node:path';

export async function gapReview({ browser, status, until, evidence, record }) {
  const before = await status();
  const text = await browser.$('[aria-label="Subtitle text"]');
  // Keep an invalid timing draft so phase-f auto-commit cannot accept it while
  // the read-only dock remains visible. The editor retains its normal policy.
  await (await browser.$('[aria-label="Cue start"]')).setValue('invalid');
  await text.setValue('Unapplied gap review draft');
  await chooseCommand(browser, 'fixGaps', { keyboard: true });
  let dock = await browser.$('[aria-label="Review panel"]');
  await until('gap review dock', () => dock.isDisplayed());
  assert.equal(
    await (await dock.$('.timing-review .apply')).isEnabled(),
    false,
  );
  await (await dock.$('button=Close')).click();
  await until('gap dock cancellation', async () => !(await dock.isExisting()));
  assert.equal(await text.getValue(), 'Unapplied gap review draft');
  assert.deepEqual(await status(), before);
  await chooseCommand(browser, 'fixGaps', { keyboard: true });
  dock = await browser.$('[aria-label="Review panel"]');
  await until('reopened gap review', () => dock.isDisplayed());
  await discardCue(browser);
  const accept = await dock.$('.timing-review .apply');
  await until(
    'completed gap proposal',
    async () =>
      (await accept.getText()) === 'Apply 1 change' &&
      (await accept.isEnabled()),
  );
  assert.deepEqual(await status(), before);
  assert.match(
    await dock.getText(),
    /No video, so shot changes weren’t checked/,
  );
  const checkbox = await dock.$('input[aria-label="Include cue 1"]');
  await checkbox.click();
  await until(
    'unchecked real gap candidate',
    async () =>
      (await accept.getText()) === 'Apply 0 changes' &&
      !(await accept.isEnabled()),
  );
  await (await dock.$('input[aria-label="Include cue 1"]')).click();
  await until(
    'checked real gap candidate',
    async () =>
      (await accept.getText()) === 'Apply 1 change' &&
      (await accept.isEnabled()),
  );
  await browser.execute(() =>
    document
      .querySelector('[aria-label="Include cue 1"]')
      .closest('.change')
      .scrollIntoView({ block: 'nearest' }),
  );
  const geometry = await browser.execute(() => {
    const bounds = (selector) => {
      const r = document.querySelector(selector).getBoundingClientRect();
      return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    };
    return {
      dock: bounds('[aria-label="Review panel"]'),
      video: bounds('[aria-label="Native video surface"]'),
      ghosts: document.querySelectorAll('.ghosts .ghost').length,
      deltas: document.querySelectorAll('.timing-delta').length,
      modal: !!document.querySelector('dialog[open]'),
    };
  });
  assert.equal(geometry.modal, false);
  assert(
    geometry.dock.top >= geometry.video.bottom ||
      geometry.dock.right <= geometry.video.left ||
      geometry.dock.left >= geometry.video.right,
    'Review dock overlaps native video bounds',
  );
  assert.equal(geometry.ghosts, 1);
  assert.equal(geometry.deltas, 1);
  await browser.saveScreenshot(join(evidence, 'gap-preview.png'));
  await accept.click();
  await until(
    'gap proposal accepted',
    async () => (await status()).revision === before.revision + 1,
  );
  await until('accepted gap workspace admission', () =>
    dock.$('button=Close').then((button) => button.isEnabled()),
  );
  await (await dock.$('button=Close')).click();
  await until(
    'accepted gap dock closed',
    async () => !(await dock.isExisting()),
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
    step: 'real live gap dock, no-media note, checked subset, bounded ghosts/deltas, atomic acceptance and Undo/Redo',
    geometry,
    changed: 1,
    originalRevision: before.revision,
    acceptedRevision: changed.revision,
    sourcePreserved: true,
  });
  return restored;
}
