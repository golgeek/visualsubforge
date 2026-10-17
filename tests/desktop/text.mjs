import assert from 'node:assert/strict';
import { join } from 'node:path';

export async function textEditing({
  browser,
  status,
  until,
  evidence,
  record,
  root,
}) {
  const original = await status();
  const editor = await browser.$('[aria-label="Subtitle text"]');
  const ready = () =>
    until('text workspace enabled', () =>
      browser.$('button=Find / Replace…').then((b) => b.isEnabled()),
    );
  async function editCue(index, text) {
    await ready();
    await (await browser.$$('.cue-row'))[index].click();
    await until(
      'selected cue ready',
      async () =>
        (await editor.getValue()) === (await status()).cues[index].text,
    );
    const before = await status();
    await editor.setValue(text);
    await (await browser.$('button=Apply edit')).click();
    await until(
      'text fixture accepted',
      async () => (await status()).revision === before.revision + 1,
    );
    await ready();
  }
  async function undo() {
    await ready();
    const before = await status();
    await (await browser.$('button=Undo edit')).click();
    await until(
      'text Undo acknowledged',
      async () => (await status()).revision === before.revision + 1,
    );
    await ready();
  }
  let dialog;
  async function open(mode, query = 'été', replacement = 'summer') {
    await ready();
    await (await browser.$('button=Find / Replace…')).click();
    dialog = await browser.$('dialog[aria-labelledby="text-title"]');
    await until('text modal', () => dialog.isDisplayed());
    await browser.execute((mode) => {
      const select = document.querySelector(
        'dialog[aria-labelledby="text-title"] select',
      );
      select.value = mode;
      select.dispatchEvent(new Event('change', { bubbles: true }));
    }, mode);
    if (mode === 'replace')
      await until('replacement field', () =>
        dialog.$('[aria-label="Replace with"]').then((e) => e.isExisting()),
      );
    if (mode === 'find' || mode === 'replace')
      await (await dialog.$('[aria-label="Find text"]')).setValue(query);
    if (mode === 'replace')
      await (
        await dialog.$('[aria-label="Replace with"]')
      ).setValue(replacement);
  }
  async function preview(mode) {
    await (
      await dialog.$(
        `button=${mode === 'find' ? 'Find matches' : 'Preview text changes'}`,
      )
    ).click();
    await until('review rows ready', async () =>
      (await dialog.getText()).includes('review rows'),
    );
  }
  async function accept(label) {
    await (await dialog.$(`button=${label}`)).click();
    await until(
      'text modal accepted',
      async () => !(await dialog.isExisting()),
    );
    await ready();
  }
  await editCue(0, '{\\an8}<i>ÉTÉ 😀 été</i>');
  await editCue(1, '<u>été</u>\nAnother line');
  const baseline = await status();
  await editor.setValue('Unapplied text-tool draft');
  await open('find');
  assert.equal(
    await (await dialog.$('button=Find matches')).isEnabled(),
    false,
  );
  await (await dialog.$('button=Cancel text tools')).click();
  await ready();
  assert.equal(await editor.getValue(), 'Unapplied text-tool draft');
  assert.deepEqual(await status(), baseline);
  await open('find');
  await (await dialog.$('button=Discard cue draft')).click();
  await preview('find');
  await (await dialog.$('button=Go to match 2 in cue 1')).click();
  await ready();
  await until('source match selected', async () =>
    browser.execute(() => {
      const e = document.querySelector('[aria-label="Subtitle text"]');
      return (
        document.activeElement === e &&
        e.value.slice(e.selectionStart, e.selectionEnd) === 'été'
      );
    }),
  );
  assert.deepEqual(await status(), baseline);
  await open('replace');
  await preview('replace');
  await browser.saveScreenshot(join(evidence, 'text-replace-preview.png'));
  await accept('Accept Replace All');
  const replaced = await status();
  assert.equal(replaced.revision, baseline.revision + 1);
  assert.deepEqual(
    replaced.cues.map((c) => c.text),
    ['{\\an8}<i>summer 😀 summer</i>', '<u>summer</u>\nAnother line'],
  );
  for (let i = 0; i < baseline.cues.length; i++) {
    assert.deepEqual(
      { ...replaced.cues[i], text: baseline.cues[i].text },
      baseline.cues[i],
    );
  }
  assert.equal(replaced.requiresSaveAs, baseline.requiresSaveAs);
  const rejection = await browser.execute(
    async (doc, path) => {
      try {
        await window.__TAURI__.core.invoke('document_action', {
          generation: doc.generation,
          revision: doc.revision,
          action: { kind: 'save', path },
        });
        return null;
      } catch (e) {
        return String(e);
      }
    },
    replaced,
    join(root, 'absent-text-parent', 'save.srt'),
  );
  assert.ok(rejection);
  assert.deepEqual(await status(), replaced);
  await undo();
  assert.deepEqual((await status()).cues, baseline.cues);
  await (await browser.$('button=Redo edit')).click();
  await until(
    'text Redo acknowledged',
    async () => (await status()).revision === replaced.revision + 2,
  );
  assert.deepEqual((await status()).cues, replaced.cues);
  await undo();
  await open('replace');
  await preview('replace');
  await (await dialog.$$('button=Replace this match'))[1].click();
  await until(
    'single replacement accepted',
    async () => !(await dialog.isExisting()),
  );
  await ready();
  assert.deepEqual(
    (await status()).cues.map((c) => c.text),
    ['{\\an8}<i>ÉTÉ 😀 summer</i>', baseline.cues[1].text],
  );
  await undo();
  for (const operation of ['joinLines', 'reflow']) {
    await open(operation);
    await preview(operation);
    if (operation === 'reflow')
      assert.ok(
        (await dialog.getText()).includes('Positioned or animated text'),
      );
    await accept('Accept text repairs');
    assert.deepEqual(
      (await status()).cues.map((c) => c.text),
      [baseline.cues[0].text, '<u>été</u> Another line'],
    );
    await undo();
  }
  await (await browser.$$('.cue-row'))[0].click();
  await until(
    'formatting cue selected',
    async () => (await editor.getValue()) === baseline.cues[0].text,
  );
  await browser.execute(() => {
    const e = document.querySelector('[aria-label="Subtitle text"]');
    e.focus();
    e.setSelectionRange(9, 12);
    e.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'b',
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
  });
  await until('formatting draft returned', async () =>
    (await editor.getValue()).includes('<b>ÉTÉ</b>'),
  );
  const formattedBefore = await status();
  assert.deepEqual(formattedBefore.cues, baseline.cues);
  await (await browser.$('button=Apply edit')).click();
  await until(
    'formatted draft accepted',
    async () => (await status()).revision === formattedBefore.revision + 1,
  );
  assert.equal(
    (await status()).cues[0].text,
    '{\\an8}<i><b>ÉTÉ</b> 😀 été</i>',
  );
  await undo();
  await undo();
  await undo();
  const restored = await status();
  assert.deepEqual(restored.cues, original.cues);
  assert.equal(restored.dirty, original.dirty);
  await record({
    step: 'real text tools: protected search, Unicode source navigation, Replace All and single match, failed save, join/reflow skips, formatting draft, atomic Undo/Redo',
    baselineRevision: baseline.revision,
    acceptedRevision: replaced.revision,
    restoredRevision: restored.revision,
    sourcePreserved: true,
  });
  return restored;
}
