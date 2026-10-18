// Real native webview, production Rust commands and isolated files. DOM-generated
// events are not physical input or GPU presentation evidence.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import {
  copyFile,
  mkdir,
  readFile,
  readdir,
  rename,
  rmdir,
  writeFile,
} from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';

export async function languageTools({
  restart,
  samePath,
  root,
  evidence,
  menu,
  invoke,
  status,
  until,
  record,
  binary,
}) {
  const report = {
    status: 'running',
    binary,
    binarySha256: createHash('sha256')
      .update(await readFile(binary))
      .digest('hex'),
    scope:
      'Actual native webview/Rust/filesystem with generated DOM events; physical input/GPU/audio acceptance remains separate.',
    cases: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'language-tools.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const media = join(root, 'language-media.mkv');
  await copyFile(resolve('tests/fixtures/generated/resync/offset.mkv'), media);
  let browser;
  const ready = () =>
    until('language workspace ready', () =>
      browser.execute(
        () =>
          document.querySelector('main')?.getAttribute('aria-busy') ===
            'false' && !document.querySelector('dialog[open]'),
      ),
    );
  const click = (label) =>
    until(`enabled ${label}`, () =>
      browser.execute((text) => {
        const button = [...document.querySelectorAll('button')].find(
          (b) => b.textContent.trim() === text && !b.disabled,
        );
        if (!button) return false;
        button.click();
        return true;
      }, label),
    );
  const input = (label, value) =>
    browser.execute(
      (label, value) => {
        const e = document.querySelector(`[aria-label="${label}"]`);
        if (!e || e.disabled) throw new Error(`Input unavailable: ${label}`);
        e.value = value;
        e.dispatchEvent(
          new Event(e.tagName === 'SELECT' ? 'change' : 'input', {
            bubbles: true,
          }),
        );
      },
      label,
      value,
    );
  const documentIs = (label, check) =>
    until(label, async () => {
      const d = await status();
      return check(d) && d;
    });
  const settings = join(root, 'settings-v1.json');
  async function spellingFilter() {
    await ready();
    await browser.execute(() => {
      const b = [...document.querySelectorAll('button')].find((e) =>
        e.textContent.trim().startsWith('Problems'),
      );
      if (b?.getAttribute('aria-expanded') !== 'true') b?.click();
    });
    await until('Problems filter', () =>
      browser.execute(
        () => !!document.querySelector('[aria-label="Problem rule"]'),
      ),
    );
    await input('Problem rule', 'spelling');
  }
  async function findings(expected) {
    await until(`settled spelling findings: ${expected}`, () =>
      browser.execute((n) => {
        const panel = document.querySelector(
          '[aria-label="Subtitle Problems"]',
        );
        return (
          panel &&
          panel.querySelector('[aria-busy]')?.getAttribute('aria-busy') ===
            'false' &&
          panel.querySelectorAll('.finding').length === n
        );
      }, expected),
    );
  }
  async function correct(word, replacement, expectedCount) {
    await findings(expectedCount);
    const before = await status();
    await click('Suggestions');
    await until('language modal', () =>
      browser.execute(
        () =>
          !!document.querySelector(
            'dialog[aria-labelledby="text-title"][open]',
          ),
      ),
    );
    await click('Get spelling suggestions');
    await click(`Preview “${replacement}”`);
    await until('language exact preview', () =>
      browser.execute(() =>
        [...document.querySelectorAll('dialog[open] button')].some(
          (b) => b.textContent.trim() === 'Accept text repairs' && !b.disabled,
        ),
      ),
    );
    assert.deepEqual(await status(), before);
    const proposal = await browser.execute(
      () => document.querySelector('[aria-label="Text proposal"]').textContent,
    );
    assert.ok(proposal.includes(word) && proposal.includes(replacement));
    await click('Accept text repairs');
    return documentIs(
      'language correction accepted',
      (d) =>
        d.revision === before.revision + 1 &&
        d.cues[0].text === before.cues[0].text.replace(word, replacement),
    );
  }
  try {
    for (const extension of ['srt', 'ass', 'ssa']) {
      for (const language of ['en-US', 'fr']) {
        const key = `${extension}-${language}`;
        const typo = language === 'fr' ? 'bonjor' : 'helo';
        const correctWord = language === 'fr' ? 'bonjour' : 'hello';
        const personalName = language === 'fr' ? 'Élodiex' : 'Shinjix';
        const line =
          extension === 'srt'
            ? `{\\an8}<i>😀 ${typo} ${typo}</i>`
            : `{\\i1}😀 ${typo} ${typo}{\\i0}`;
        const template =
          extension === 'srt'
            ? '\uFEFF7\r\n00:00:01,001 --> 00:00:03,999\r\nFIRST\r\n\r\n9\r\n00:00:04,100 --> 00:00:06,000\r\nSECOND\r\n\r\n'
            : await readFile(
                resolve(`tests/fixtures/subtitles/styles.${extension}`),
                'utf8',
              );
        const original = template
          .replace(extension === 'srt' ? 'FIRST' : 'Red title', line)
          .replace(
            extension === 'srt' ? 'SECOND' : 'Second line\\N世界',
            '{\\p1}m 0 0 l 5 5',
          );
        const source = join(root, `language-${key}.${extension}`);
        const saved = join(root, `saved-language-${key}.${extension}`);
        const recovered = join(root, `recovered-language-${key}.${extension}`);
        const project = join(root, `language-${key}.vsfproject`);
        await writeFile(source, original);
        browser = await restart([
          { kind: 'media', path: media },
          { kind: 'subtitles', path: source },
          {
            kind: 'saveSubtitles',
            path: join(root, 'missing-language', `bad.${extension}`),
          },
          { kind: 'saveSubtitles', path: saved },
          { kind: 'subtitles', path: saved },
          { kind: 'saveProject', path: project },
          { kind: 'project', path: project },
        ]);
        await ready();
        await menu('openMedia');
        await until(
          'language project media loaded',
          async () =>
            (
              await invoke('media_action', {
                action: { kind: 'status' },
                session: 0,
              })
            ).loaded,
        );
        await ready();
        await menu('openSubtitles');
        const baseline = await documentIs(
          'language source loaded',
          (d) => samePath(d.path, source) && d.cues.length === 2,
        );
        await ready();
        const preferences = await invoke('preferences_get');
        const context = (await invoke('quality_settings')).current;
        preferences.quality = {
          ...preferences.quality,
          id: `language-${key}`,
          name: `Language ${key}`,
          limitations: [],
          languageTools: {
            dictionary: language,
            spelling: false,
            typography: false,
            capitalization: false,
            dialogueMarkers: false,
          },
        };
        preferences.quality.rules.duration.enabled = false;
        preferences.quality.rules.speed.enabled = false;
        preferences.quality.rules.lines.enabled = false;
        preferences.quality.rules.gaps = false;
        preferences.quality.timing.sceneBeforeMs =
          preferences.quality.timing.sceneAfterMs = 0;
        preferences.timing = {
          ...preferences.timing,
          ...preferences.quality.timing,
        };
        preferences.personalWords = { englishUs: [], french: [] };
        await invoke('preferences_save', { preferences, epoch: context.epoch });
        await menu('settings');
        await click('Customize a copy');
        await input('Language dictionary', language);
        await browser.execute(() => {
          const e = [...document.querySelectorAll('label')]
            .find((l) => l.textContent.trim() === 'Check spelling')
            ?.querySelector('input');
          if (!e || e.disabled || e.checked)
            throw new Error('Expected editable, disabled spelling rule');
          e.click();
        });
        await click('Read bundled license notices');
        await until('bundled notices loaded', () =>
          browser.execute(() =>
            [...document.querySelectorAll('dialog pre')].some((p) =>
              p.textContent.includes('Mozilla Public License'),
            ),
          ),
        );
        await click('Apply settings');
        await ready();
        const policy = (await invoke('quality_settings')).current.profile;
        assert.equal(policy.languageTools.dictionary, language);
        assert.equal(policy.languageTools.spelling, true);
        await spellingFilter();
        await findings(2);
        const edited = await correct(typo, correctWord, 2);
        assert.deepEqual(edited.cues[1], baseline.cues[1]);
        assert.deepEqual(
          { ...edited.cues[0], text: baseline.cues[0].text },
          baseline.cues[0],
        );
        await ready();
        await click('Undo edit');
        await documentIs(
          'native language Undo',
          (d) => d.revision === edited.revision + 1 && d.cues[0].text === line,
        );
        await ready();
        await click('Redo edit');
        await documentIs(
          'native language Redo',
          (d) =>
            d.revision === edited.revision + 2 &&
            d.cues[0].text === edited.cues[0].text,
        );
        await ready();
        await menu('saveAs');
        await until('language failed save reported', () =>
          browser.execute(() =>
            /Cannot|cannot|No such|not find/.test(
              document.querySelector('footer [role="status"]')?.textContent ??
                '',
            ),
          ),
        );
        assert.equal((await status()).dirty, true);
        assert.equal(await readFile(source, 'utf8'), original);
        await ready();
        await menu('saveAs');
        await documentIs(
          'language Save As',
          (d) => samePath(d.path, saved) && !d.dirty,
        );
        const savedBytes = original.replace(typo, correctWord);
        assert.equal(await readFile(saved, 'utf8'), savedBytes);
        await ready();
        await menu('closeSubtitles');
        await documentIs('language close', (d) => !d.loaded);
        await ready();
        await menu('openSubtitles');
        await documentIs(
          'language reopen',
          (d) =>
            samePath(d.path, saved) && d.cues[0].text === edited.cues[0].text,
        );
        await ready();
        await menu('saveProject');
        const projectData = await until('language project saved', async () => {
          try {
            return JSON.parse(await readFile(project, 'utf8'));
          } catch (e) {
            if (e.code === 'ENOENT') return false;
            throw e;
          }
        });
        assert.deepEqual(
          projectData.options.quality.languageTools,
          policy.languageTools,
        );
        assert.equal(
          Object.hasOwn(projectData.options.quality, 'personalWords'),
          false,
        );
        await ready();
        await menu('closeSubtitles');
        await documentIs(
          'closed before language project restore',
          (d) => !d.loaded,
        );
        await ready();
        await menu('openProject');
        await documentIs('language project restored', (d) =>
          samePath(d.path, saved),
        );
        assert.deepEqual(
          (await invoke('quality_settings')).current.profile.languageTools,
          policy.languageTools,
        );
        await spellingFilter();
        await findings(1);
        await click('Ignore word this session');
        await findings(0);
        let words = await invoke('language_words');
        assert.deepEqual(
          words.ignored[language === 'fr' ? 'french' : 'englishUs'],
          [typo],
        );
        await click('Clear session ignored words');
        await findings(1);
        const beforeWords = await invoke('language_words');
        const backup = join(root, `settings-${key}.backup`);
        await rename(settings, backup);
        await mkdir(settings);
        await click('Add personal word');
        await until('native personal-word persistence failure', () =>
          browser.execute(
            () =>
              !!document.querySelector(
                '[aria-label="Subtitle Problems"] [role="alert"]',
              ),
          ),
        );
        assert.deepEqual(await invoke('language_words'), beforeWords);
        await rmdir(settings);
        await rename(backup, settings);
        await click('Add personal word');
        await findings(0);
        assert.deepEqual(
          JSON.parse(await readFile(settings, 'utf8')).personalWords[
            language === 'fr' ? 'french' : 'englishUs'
          ],
          [typo],
        );
        await ready();
        await menu('settings');
        await until('personal words loaded', () =>
          browser.execute(
            () =>
              !!document.querySelector('[aria-label="French personal words"]'),
          ),
        );
        await input(
          language === 'fr'
            ? 'French personal words'
            : 'English (US) personal words',
          personalName,
        );
        await click('Apply settings');
        await ready();
        await spellingFilter();
        const recoveryEdit = await correct(typo, correctWord, 1);
        const expected = original.replaceAll(typo, correctWord);
        await until('durable language correction checkpoint', async () => {
          for (const entry of await readdir(join(root, 'recovery-v1'), {
            withFileTypes: true,
          })) {
            if (!entry.isDirectory()) continue;
            try {
              const data = JSON.parse(
                await readFile(
                  join(root, 'recovery-v1', entry.name, 'snapshot.json'),
                  'utf8',
                ),
              );
              if (
                data.version === (extension === 'srt' ? 3 : 4) &&
                data.accepted_srt === expected
              )
                return true;
            } catch (e) {
              if (e.code !== 'ENOENT') throw e;
            }
          }
          return false;
        });
        words = await invoke('language_words');
        await invoke('language_words_change', {
          epoch: words.epoch,
          change: { kind: 'ignore', language, word: 'Sessionword' },
        });
        const external = savedBytes.replace(correctWord, 'External newer edit');
        await writeFile(saved, external);
        await browser.saveScreenshot(
          join(evidence, `language-${key}-accepted.png`),
        );
        browser = await restart([
          { kind: 'saveSubtitles', path: saved },
          { kind: 'saveSubtitles', path: recovered },
        ]);
        const row = await browser.$(
          `//div[contains(@class,"recovery-row")][span[starts-with(normalize-space(.),"${basename(saved)} ")]]`,
        );
        await until('language recovery entry', () => row.isExisting());
        await (await row.$('button=Recover')).click();
        const recoveredDoc = await documentIs(
          'language correction recovered',
          (d) =>
            d.requiresSaveAs && d.cues[0]?.text === recoveryEdit.cues[0].text,
        );
        assert.equal(recoveredDoc.format?.name ?? 'srt', extension);
        assert.equal(recoveredDoc.canUndo, false);
        words = await invoke('language_words');
        assert.deepEqual(
          words.personal[language === 'fr' ? 'french' : 'englishUs'],
          [personalName],
        );
        assert.deepEqual(words.ignored, { englishUs: [], french: [] });
        await ready();
        await menu('saveAs');
        await until('language external conflict preserved', () =>
          browser.execute(() =>
            /changed on disk/.test(
              document.querySelector('footer [role="status"]')?.textContent ??
                '',
            ),
          ),
        );
        assert.equal(await readFile(saved, 'utf8'), external);
        await ready();
        await menu('saveAs');
        await documentIs(
          'separate language recovery copy',
          (d) => samePath(d.path, recovered) && !d.dirty,
        );
        assert.equal(await readFile(recovered, 'utf8'), expected);
        assert.equal(await readFile(source, 'utf8'), original);
        // Exercise each optional editorial rule through the actual Settings,
        // Problems and reviewed text UI, keeping every correction independent.
        await ready();
        await menu('settings');
        await click('Customize a copy');
        await browser.execute(() => {
          for (const label of [
            'Check spelling',
            'Check typography',
            'Check conservative sentence capitalization',
            'Check existing dialogue markers',
          ]) {
            const e = [...document.querySelectorAll('label')]
              .find((l) => l.textContent.trim() === label)
              ?.querySelector('input');
            if (!e || e.disabled) throw new Error(`Missing rule: ${label}`);
            if (e.checked !== (label !== 'Check spelling')) e.click();
          }
        });
        await click('Apply settings');
        await ready();
        const visible = language === 'fr' ? '-bonjour...' : '—hello...';
        const editorialLine =
          extension === 'srt'
            ? `{\\an8}<i>${visible}</i>`
            : `{\\i1}${visible}{\\i0}`;
        const savedState = await status();
        await input('Subtitle text', editorialLine);
        await click('Apply edit');
        await documentIs(
          'editorial source accepted',
          (d) => d.cues[0].text === editorialLine,
        );
        await spellingFilter();
        for (const [rule, from, to] of [
          ['typography', '...', '…'],
          [
            'capitalization',
            correctWord,
            language === 'fr' ? 'Bonjour' : 'Hello',
          ],
          [
            'dialogue_marker',
            language === 'fr' ? '-' : '—',
            language === 'fr' ? '— ' : '- ',
          ],
        ]) {
          await input('Problem rule', rule);
          await findings(1);
          const before = await status();
          await click('Review correction');
          await click('Preview text changes');
          await until('editorial correction review ready', () =>
            browser.execute(() =>
              [...document.querySelectorAll('dialog button')].some(
                (b) =>
                  b.textContent.trim() === 'Accept text repairs' && !b.disabled,
              ),
            ),
          );
          assert.deepEqual(await status(), before);
          await click('Accept text repairs');
          const accepted = await documentIs(
            'editorial correction accepted',
            (d) => d.revision === before.revision + 1,
          );
          assert.deepEqual(
            accepted.cues,
            before.cues.map((c, i) =>
              i === 0 ? { ...c, text: editorialLine.replace(from, to) } : c,
            ),
          );
          await ready();
          await click('Undo edit');
          await documentIs(
            'editorial correction Undo',
            (d) => d.revision === before.revision + 2,
          );
          assert.deepEqual((await status()).cues, before.cues);
          await ready();
          await click('Redo edit');
          await documentIs(
            'editorial correction Redo',
            (d) => d.revision === before.revision + 3,
          );
          assert.deepEqual((await status()).cues, accepted.cues);
          await ready();
          await click('Undo edit');
          await documentIs(
            'editorial correction restored',
            (d) => d.revision === before.revision + 4,
          );
          await ready();
        }
        await click('Undo edit');
        await documentIs('saved language fixture restored', (d) => !d.dirty);
        assert.deepEqual((await status()).cues, savedState.cues);
        report.cases.push({
          extension,
          language,
          sourceSha256: createHash('sha256').update(original).digest('hex'),
          exactSourcePreservation: true,
          suggestionsAndReviewedUndo: true,
          failedSavePreserved: true,
          projectPolicyRestored: true,
          personalPersistence: true,
          sessionIgnores: true,
          recoveryAndExternalConflict: true,
          editorialRulesReviewedUndoRedo: [
            'typography',
            'capitalization',
            'dialogue_marker',
          ],
        });
        await save();
        await record({
          step: `native language ${key}: review/history/save/project/words/recovery`,
        });
      }
    }
    report.status = 'passed';
    await save();
  } catch (error) {
    report.status = 'failed';
    report.error = String(error);
    await save();
    throw error;
  }
}
