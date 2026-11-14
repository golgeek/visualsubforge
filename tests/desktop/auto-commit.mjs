import assert from 'node:assert/strict';

// Actual Rust IPC/history, with generated DOM typing; no history mock or direct
// edit command. Leave the document's content and saved identity as found.
export async function autoCommit({ browser, status, until, record }) {
  const original = await status();
  const cue = original.cues[0];
  await (await browser.$$('.cue-row'))[0].click();
  await until(
    'auto-commit cue selected',
    async () =>
      (await (await browser.$('[aria-label="Subtitle text"]')).getValue()) ===
      cue.text,
  );
  const type = (value) =>
    browser.execute((value) => {
      const editor = document.querySelector('[aria-label="Subtitle text"]');
      editor.focus();
      editor.value = value;
      editor.dispatchEvent(new Event('input', { bubbles: true }));
    }, value);
  await type('First paused text');
  await until(
    'first pause accepted',
    async () => (await status()).cues[0].text === 'First paused text',
  );
  await type('Second paused text');
  await until(
    'second pause accepted',
    async () => (await status()).cues[0].text === 'Second paused text',
  );
  assert.equal((await status()).revision, original.revision + 2);
  await (await browser.$('button=Undo edit')).click();
  await until(
    'same-focus text coalesces into one Undo',
    async () => (await status()).cues[0].text === cue.text,
  );
  assert.deepEqual((await status()).cues, original.cues);
  assert.equal((await status()).dirty, original.dirty);
  await (await browser.$('button=Redo edit')).click();
  await until(
    'coalesced Redo restores latest text',
    async () => (await status()).cues[0].text === 'Second paused text',
  );
  await (await browser.$('button=Undo edit')).click();
  await until(
    'auto-commit trial restores original document',
    async () => (await status()).cues[0].text === cue.text,
  );
  await record({
    step: 'two real pause commits coalesce to one exact Undo and Redo',
    mockedIpc: false,
    physicalInput: false,
    revisions: (await status()).revision - original.revision,
  });
  return status();
}
