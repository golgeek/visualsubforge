// Real desktop owner/files/player; generated DOM input is distinct from physical
// input. Optional native-window observations inspect sibling video composition.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { copyFile, readFile, readdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
function encoded(bytes) {
  let out = '';
  for (let i = 0; i < bytes.length; i += 3) {
    const value =
      (bytes[i] << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0);
    for (const shift of [18, 12, 6, 0].slice(
      0,
      Math.min(3, bytes.length - i) + 1,
    ))
      out += String.fromCharCode(33 + ((value >>> shift) & 63));
  }
  return out.match(/.{1,80}/g).join('\r\n') + '\r\n';
}
export async function fontAuthoring({
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
    binarySha256: hash(await readFile(binary)),
    scope:
      'Actual webview/Rust/files and native preview acknowledgments. Generated DOM events; optional separate native-window observations. Physical Windows/Linux GPU/audio and installation acceptance remain open.',
    cases: [],
    captures: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'font-authoring.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const ttf = await readFile(
    resolve('tests/fixtures/subtitles/fixture-font.ttf'),
  );
  const otf = await readFile(
    resolve('tests/fixtures/subtitles/fixture-font-cff.otf'),
  );
  const first = join(root, 'Fixture.ttf'),
    second = join(root, 'Changed.otf'),
    bad = join(root, 'Invalid.ttf'),
    media = join(root, 'font-video.mkv');
  await writeFile(first, ttf);
  await writeFile(second, otf);
  await writeFile(bad, 'Invalid font');
  await copyFile(resolve('tests/fixtures/generated/resync/offset.mkv'), media);
  let browser;
  const ready = () =>
    until('font workspace ready', () =>
      browser.execute(
        () =>
          document.querySelector('main')?.getAttribute('aria-busy') ===
            'false' && !document.querySelector('dialog[open]'),
      ),
    );
  const click = (label) =>
    until(`enabled ${label}`, () =>
      browser.execute((label) => {
        const button = [...document.querySelectorAll('button')].find(
          (b) => b.textContent.trim() === label && !b.disabled,
        );
        if (!button) return false;
        button.click();
        return true;
      }, label),
    );
  const documentIs = (label, check) =>
    until(label, async () => {
      const d = await status();
      return check(d) && d;
    });
  const state = () =>
    invoke('media_action', { action: { kind: 'status' }, session: 0 });
  async function openFonts() {
    await ready();
    await click('Manage fonts…');
    await until('font inventory ready', () =>
      browser.execute(() =>
        document
          .querySelector('dialog[aria-labelledby="font-dialog-title"][open]')
          ?.textContent.includes('Font review: reviewing'),
      ),
    );
  }
  async function review(label) {
    await click(label);
    await until('font review ready', () =>
      browser.execute(
        () =>
          !!document.querySelector('[aria-label="Font change review"]') &&
          [...document.querySelectorAll('button')].some(
            (b) => b.textContent.trim() === 'Accept font change' && !b.disabled,
          ),
      ),
    );
  }
  async function accept() {
    const before = await status();
    await click('Accept font change');
    const after = await documentIs(
      'font accepted',
      (d) => d.revision === before.revision + 1,
    );
    await ready();
    return after;
  }
  async function preview(label) {
    const doc = await status(),
      current = await state();
    await invoke('media_action', {
      action: { kind: 'seek', seconds: 1.5 },
      session: current.session,
    });
    const m = await until('native font preview current', async () => {
      const m = await state();
      assert.equal(m.subtitlePreviewError, null);
      assert.equal(m.error, null);
      return (
        m.loaded &&
        !m.seeking &&
        m.renderedFrames > 0 &&
        m.previewDocumentGeneration === doc.generation &&
        m.previewDocumentRevision === doc.revision &&
        m
      );
    });
    const capture = {
      label,
      documentGeneration: doc.generation,
      documentRevision: doc.revision,
      media: m,
    };
    report.captures.push(capture);
    await save();
    await browser.saveScreenshot(join(evidence, `${label}-webview.png`));
    const labels = process.env.VSF_FONT_DESKTOP_CAPTURE_LABELS?.split(',');
    if (
      process.env.VSF_FONT_DESKTOP_CAPTURE === '1' &&
      (!labels || labels.includes(label))
    ) {
      await writeFile(
        join(evidence, 'capture-request.json'),
        JSON.stringify(capture, null, 2),
      );
      console.log(`Native font capture ready: ${label}; evidence: ${evidence}`);
      await until(
        `native font capture ${label}`,
        async () => {
          try {
            return (
              JSON.parse(
                await readFile(join(evidence, 'capture-ack.json'), 'utf8'),
              ).label === label
            );
          } catch (e) {
            if (e.code === 'ENOENT') return false;
            throw e;
          }
        },
        60000,
      );
    }
    return m;
  }
  try {
    for (const extension of ['ass', 'ssa']) {
      const source = join(root, `font-source.${extension}`),
        copy = join(root, `font-copy.${extension}`),
        recovered = join(root, `font-recovered.${extension}`),
        project = join(root, `font-${extension}.vsfproject`);
      const base = await readFile(
        resolve(`tests/fixtures/subtitles/styles.${extension}`),
        'utf8',
      );
      const original =
        '\uFEFF' +
        base
          .replaceAll('sans-serif', 'VSF Fixture Font')
          .replace('Title,VSF Fixture Font,36', 'Title,VSF Fixture Font,96')
          .replace('Red title', 'AB')
          .replace('Second line\\N世界', 'AB')
          .replaceAll('\n', '\r\n');
      const added = `${original}[Fonts]\r\nfontname: Fixture.ttf\r\n${encoded(ttf)}`;
      const changed = `${original}[Fonts]\r\nfontname: Changed.otf\r\n${encoded(otf)}`;
      await writeFile(source, original);
      browser = await restart([
        { kind: 'media', path: media },
        { kind: 'subtitles', path: source },
        { kind: 'font', path: null },
        { kind: 'font', path: bad },
        { kind: 'font', path: first },
        { kind: 'font', path: second },
        {
          kind: 'saveSubtitles',
          path: join(root, 'missing-parent', `font.${extension}`),
        },
        { kind: 'saveSubtitles', path: copy },
        { kind: 'saveProject', path: project },
        { kind: 'subtitles', path: copy },
        { kind: 'project', path: project },
      ]);
      await menu('openMedia');
      await until('font media loaded', async () => (await state()).loaded);
      await ready();
      await menu('openSubtitles');
      const opened = await documentIs('font source opened', (d) =>
        samePath(d.path, source),
      );
      await ready();
      await openFonts();
      await click('Add font…');
      await until('font picker cancelled', () =>
        browser.execute(() =>
          [...document.querySelectorAll('button')].some(
            (b) => b.textContent.trim() === 'Add font…' && !b.disabled,
          ),
        ),
      );
      assert.equal((await status()).revision, opened.revision);
      await click('Add font…');
      await until('invalid font refused', () =>
        browser.execute(() =>
          document
            .querySelector('dialog[open]')
            ?.textContent.includes('Font review: failed'),
        ),
      );
      assert.equal((await status()).dirty, false);
      await review('Add font…');
      assert.equal((await status()).dirty, false);
      assert.match(
        await browser.execute(
          () =>
            document.querySelector('[aria-label="Font change review"]')
              .textContent,
        ),
        new RegExp(hash(ttf)),
      );
      await browser.saveScreenshot(
        join(evidence, `font-${extension}-add-review.png`),
      );
      const accepted = await accept();
      assert.deepEqual(accepted.cues, opened.cues);
      const initial = await preview(`font-${extension}-ttf`);
      await click('Undo edit');
      await documentIs(
        'font add undone',
        (d) => d.revision === accepted.revision + 1 && !d.dirty,
      );
      await ready();
      await click('Redo edit');
      await documentIs(
        'font add redone',
        (d) => d.revision === accepted.revision + 2 && d.dirty,
      );
      await ready();
      await openFonts();
      await review('Replace font…');
      const reviewText = await browser.execute(
        () =>
          document.querySelector('[aria-label="Font change review"]')
            .textContent,
      );
      assert.ok(
        reviewText.includes(hash(ttf)) && reviewText.includes(hash(otf)),
      );
      await browser.saveScreenshot(
        join(evidence, `font-${extension}-replace-review.png`),
      );
      await accept();
      const replacement = await preview(`font-${extension}-otf`);
      assert.equal(replacement.session, initial.session);
      await openFonts();
      await review('Preview removal');
      await accept();
      const removed = await preview(`font-${extension}-removed`);
      assert.equal(removed.session, initial.session);
      await click('Undo edit');
      await documentIs('font removal undone', (d) => d.dirty);
      await ready();
      await preview(`font-${extension}-undo`);
      await menu('saveAs');
      await until('failed font save', () =>
        browser.execute(() =>
          /Cannot|cannot|No such/.test(
            document.querySelector('footer [role="status"]')?.textContent ?? '',
          ),
        ),
      );
      await ready();
      assert.equal(await readFile(source, 'utf8'), original);
      await menu('saveAs');
      await documentIs(
        'font copy saved',
        (d) => !d.dirty && samePath(d.path, copy),
      );
      await ready();
      assert.equal(await readFile(copy, 'utf8'), changed);
      assert.equal(await readFile(source, 'utf8'), original);
      await menu('saveProject');
      await until('font project saved', async () => {
        try {
          return (
            JSON.parse(await readFile(project, 'utf8')).subtitleFormat ===
            extension
          );
        } catch (e) {
          if (e.code === 'ENOENT') return false;
          throw e;
        }
      });
      await ready();
      await menu('closeSubtitles');
      await documentIs('font copy closed', (d) => !d.loaded);
      await ready();
      await menu('openSubtitles');
      const reopened = await documentIs('font copy reopened', (d) =>
        samePath(d.path, copy),
      );
      await ready();
      assert.deepEqual(reopened.cues, opened.cues);
      await openFonts();
      assert.ok(
        (
          await browser.execute(
            () =>
              document.querySelector('[aria-label="Attachment details"]')
                .textContent,
          )
        ).includes(hash(otf)),
      );
      await click('Close fonts');
      await ready();
      await menu('closeSubtitles');
      await documentIs('closed for font project', (d) => !d.loaded);
      await ready();
      await menu('openProject');
      await documentIs('font project restored', (d) => samePath(d.path, copy));
      await ready();
      await preview(`font-${extension}-project`);
      await openFonts();
      await review('Preview removal');
      await accept();
      const recoveryBytes = `${original}[Fonts]\r\n`;
      await until('font source-property checkpoint', async () => {
        for (const entry of await readdir(join(root, 'recovery-v1'), {
          withFileTypes: true,
        })) {
          if (!entry.isDirectory()) continue;
          try {
            const c = JSON.parse(
              await readFile(
                join(root, 'recovery-v1', entry.name, 'snapshot.json'),
                'utf8',
              ),
            );
            if (c.version === 4 && c.accepted_srt === recoveryBytes)
              return true;
          } catch (e) {
            if (e.code !== 'ENOENT') throw e;
          }
        }
        return false;
      });
      await writeFile(copy, changed + '; External newer edit\r\n');
      browser = await restart([
        { kind: 'saveSubtitles', path: copy },
        { kind: 'saveSubtitles', path: recovered },
      ]);
      const row = await browser.$(
        `//div[contains(@class,"recovery-row")][span[starts-with(normalize-space(.),"font-copy.${extension} ")]]`,
      );
      await until('font recovery entry', () => row.isExisting());
      await (await row.$('button=Recover')).click();
      const recovery = await documentIs(
        'font change recovered',
        (d) => d.requiresSaveAs && d.format?.name === extension,
      );
      assert.equal(recovery.canUndo, false);
      await ready();
      await menu('saveAs');
      await until('font external conflict', () =>
        browser.execute(() =>
          document
            .querySelector('footer [role="status"]')
            ?.textContent.includes('changed on disk'),
        ),
      );
      await ready();
      assert.equal(
        await readFile(copy, 'utf8'),
        changed + '; External newer edit\r\n',
      );
      await menu('saveAs');
      await documentIs(
        'recovered font change saved separately',
        (d) => !d.dirty && samePath(d.path, recovered),
      );
      await ready();
      assert.equal(await readFile(recovered, 'utf8'), recoveryBytes);
      report.cases.push({
        extension,
        sourceSha256: hash(original),
        addedSha256: hash(added),
        replacedSha256: hash(changed),
        recoveredSha256: hash(recoveryBytes),
        samePlayerSession: initial.session,
        cuePreservation: true,
        invalidRefused: true,
        pickerCancelled: true,
        addReplaceRemove: true,
        undoRedo: true,
        exactSaveReopen: true,
        failedSavePreserved: true,
        projectRestore: true,
        propertyOnlyCrashRecovery: true,
        externalConflictPreserved: true,
      });
      await save();
      await record({
        step: `actual ${extension} font authoring, same-player replacement, save, history, project and recovery`,
      });
    }
    report.status = 'passed';
    await save();
  } catch (e) {
    report.status = 'failed';
    report.error = String(e.stack ?? e);
    await save();
    throw e;
  }
}
