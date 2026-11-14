import { commitCue, discardCue } from './editor-input.mjs';
import assert from 'node:assert/strict';

export async function editReconciliation({
  browser,
  invoke,
  status,
  until,
  record,
}) {
  const original = await status();
  const cue = original.cues[0];
  const editor = await browser.$('[aria-label="Subtitle text"]');
  const ready = () =>
    until('reconciliation workspace ready', () =>
      browser.$('button=Commands').then((b) => b.isEnabled()),
    );
  await ready();
  await (await browser.$$('.cue-row'))[0].click();
  await editor.setValue('Retained draft');
  // Deliberately bypass DocumentSession to test a defensive stale-response path.
  // This is not evidence of a second writer in the production single-window app.
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
  await commitCue(browser);
  await until('stale state refreshed without replay', () =>
    browser
      .$('footer [role="status"]')
      .then((e) => e.getText())
      .then((text) => text.includes('Current subtitles refreshed')),
  );
  await ready();
  assert.equal((await status()).revision, concurrent.revision);
  assert.equal(await editor.getValue(), 'Retained draft');
  assert.equal(await (await browser.$('#cue-commit-error')).isExisting(), true);
  await discardCue(browser);
  assert.equal(await editor.getValue(), cue.text);
  await (await browser.$('button=Undo edit')).click();
  await until(
    'only injected edit in history',
    async () => (await status()).revision === concurrent.revision + 1,
  );
  await ready();
  assert.deepEqual((await status()).cues, original.cues);
  await record({
    step: 'injected stale refusal refreshes and retains draft without replay or added history',
    mockedIpc: false,
    productionSecondWriter: false,
  });
  return status();
}
