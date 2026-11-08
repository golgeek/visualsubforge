import assert from 'node:assert/strict';

export async function editRetry({ browser, invoke, status, until, record }) {
  const original = await status();
  const cue = original.cues[0];
  const editor = await browser.$('[aria-label="Subtitle text"]');
  const ready = () =>
    until('retry workspace ready', () =>
      browser.$('button=Find / Replace…').then((b) => b.isEnabled()),
    );
  await ready();
  await (await browser.$$('.cue-row'))[0].click();
  await editor.setValue('Retry draft');
  const concurrent = await invoke('document_action', {
    action: {
      kind: 'edit',
      cueId: cue.id,
      startMs: cue.startMs,
      endMs: cue.endMs + 10,
      text: cue.text,
    },
    generation: original.generation,
    revision: original.revision,
  });
  const refusal = await browser.execute(
    async (version) => {
      try {
        await window.__TAURI__.core.invoke('document_action', {
          action: { kind: 'undo' },
          ...version,
        });
        return null;
      } catch (error) {
        return error;
      }
    },
    { generation: original.generation, revision: original.revision },
  );
  assert.equal(refusal.kind, 'staleRevision');
  assert.equal(typeof refusal.message, 'string');
  await (await browser.$('button=Apply edit')).click();
  await until(
    'stale edit retried',
    async () => (await status()).revision === concurrent.revision + 1,
  );
  await ready();
  const accepted = await status();
  assert.equal(accepted.cues[0].text, 'Retry draft');
  assert.equal(accepted.cues[0].endMs, cue.endMs + 10);
  assert.deepEqual(accepted.cues.slice(1), original.cues.slice(1));
  async function undo(expected) {
    const before = await status();
    await (await browser.$('button=Undo edit')).click();
    await until(
      'retry Undo acknowledged',
      async () => (await status()).revision === before.revision + 1,
    );
    await ready();
    assert.deepEqual((await status()).cues, expected);
  }
  await undo(concurrent.cues);
  await undo(original.cues);
  const restored = await status();
  await editor.setValue('Conflicting draft');
  const competing = await invoke('document_action', {
    action: {
      kind: 'edit',
      cueId: cue.id,
      startMs: cue.startMs,
      endMs: cue.endMs,
      text: 'Competing text',
    },
    generation: restored.generation,
    revision: restored.revision,
  });
  await (await browser.$('button=Apply edit')).click();
  await until('conflict retained', () =>
    browser.execute(() =>
      [...document.querySelectorAll('[role="alert"]')].some((e) =>
        e.textContent.includes('Draft retained'),
      ),
    ),
  );
  assert.equal((await status()).revision, competing.revision);
  assert.equal(await editor.getValue(), 'Conflicting draft');
  assert.equal(await (await browser.$('button=Apply edit')).isEnabled(), false);
  await (await browser.$('button=Revert draft')).click();
  assert.equal(await editor.getValue(), 'Competing text');
  await undo(original.cues);
  await record({
    step: 'typed stale rejection, guarded automatic edit retry, one retry history entry and retained competing draft',
    mockedIpc: false,
  });
  return status();
}
