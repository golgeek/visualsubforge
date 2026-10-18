// Actual webview/Rust/filesystem review. Generated DOM input is not physical
// interaction or compositor-presentation evidence; runtime identity is retained.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { copyFile, readFile, readdir, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

export async function styleAuthoring({
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
      'Actual native webview, Rust, files and accepted native subtitle preview; generated DOM events. Physical input, GPU/audio and installation acceptance remain separate.',
    cases: [],
  };
  const save = () =>
    writeFile(
      join(evidence, 'style-authoring.json'),
      JSON.stringify(report, null, 2) + '\n',
    );
  const media = join(root, 'style-video.mkv');
  await copyFile(resolve('tests/fixtures/generated/resync/offset.mkv'), media);
  let browser;
  const ready = async () => {
    await browser.execute(
      () =>
        new Promise((r) =>
          requestAnimationFrame(() => requestAnimationFrame(r)),
        ),
    );
    await until('style workspace ready', () =>
      browser.execute(
        () =>
          document.querySelector('main')?.getAttribute('aria-busy') ===
            'false' && !document.querySelector('dialog[open]'),
      ),
    );
  };
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
  const state = () =>
    invoke('media_action', { action: { kind: 'status' }, session: 0 });
  const style = (d, name, field) =>
    d.format.styles
      .find((s) => s.name === name)
      ?.fields.find(([f]) => f === field)?.[1];
  async function preview(label) {
    const doc = await status(),
      current = await state();
    await invoke('media_action', {
      action: { kind: 'seek', seconds: 1.5 },
      session: current.session,
    });
    const m = await until('native style preview current', async () => {
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
    await browser.saveScreenshot(join(evidence, `${label}.png`));
    return m;
  }
  async function openStyle(name, mode = 'Edit style') {
    await ready();
    await click('Inspect styles…');
    await until('style inspector', () =>
      browser.execute(
        () =>
          !!document.querySelector(
            'dialog[aria-labelledby="style-inspector-title"][open]',
          ),
      ),
    );
    if (name) await input('Inspect style', name);
    await click(mode);
    await until('style form', () =>
      browser.execute(
        () => !!document.querySelector('[aria-label="Style Name"]'),
      ),
    );
  }
  async function review() {
    await click('Preview style changes');
    await until('style review ready', () =>
      browser.execute(
        () =>
          !!document.querySelector('[aria-label="Style change review"]') &&
          [...document.querySelectorAll('button')].some(
            (b) =>
              b.textContent.trim() === 'Accept style changes' && !b.disabled,
          ),
      ),
    );
  }
  async function accept() {
    const before = await status();
    await click('Accept style changes');
    const after = await documentIs(
      'style accepted',
      (d) => d.revision === before.revision + 1,
    );
    await ready();
    return after;
  }
  try {
    for (const extension of ['ass', 'ssa']) {
      const source = join(root, `style-source.${extension}`),
        copy = join(root, `style-copy.${extension}`),
        recovered = join(root, `style-recovered.${extension}`),
        project = join(root, `style-${extension}.vsfproject`);
      const base = await readFile(
        resolve(`tests/fixtures/subtitles/styles.${extension}`),
        'utf8',
      );
      const original =
        '\uFEFF' +
        base
          .replace('Red title', '{\\rTitle}Red title')
          .replace('Comment: Default,', 'Comment: Title,')
          .replaceAll('\n', '\r\n');
      await writeFile(source, original);
      browser = await restart([
        { kind: 'media', path: media },
        { kind: 'subtitles', path: source },
        {
          kind: 'saveSubtitles',
          path: join(root, 'missing-parent', `style.${extension}`),
        },
        { kind: 'saveSubtitles', path: copy },
        { kind: 'saveProject', path: project },
        { kind: 'subtitles', path: copy },
        { kind: 'project', path: project },
      ]);
      await menu('openMedia');
      await until('style media loaded', async () => (await state()).loaded);
      await ready();
      await menu('openSubtitles');
      const opened = await documentIs('style source opened', (d) =>
        samePath(d.path, source),
      );
      await ready();
      assert.equal(style(opened, 'Title', 'Fontsize'), '36');
      // Copy and canonical creation use the real form/proposal/history path.
      for (const [mode, name] of [
        ['Copy style', 'Copied'],
        ['New style', 'Created'],
      ]) {
        await openStyle('Title', mode);
        await input('Style Name', name);
        await review();
        assert.deepEqual((await status()).format, opened.format);
        const created = await accept();
        assert.ok(created.format.styles.some((s) => s.name === name));
        assert.deepEqual(created.cues, opened.cues);
        await click('Undo edit');
        await documentIs(
          'created style undone',
          (d) => !d.format.styles.some((s) => s.name === name),
        );
        await ready();
      }
      await openStyle('Title');
      await input('Style Fontsize', '0');
      await click('Preview style changes');
      await until('invalid style error', () =>
        browser.execute(() =>
          document
            .querySelector('dialog[open]')
            ?.textContent.includes('Style preview: failed'),
        ),
      );
      assert.equal(style(await status(), 'Title', 'Fontsize'), '36');
      await input('Style Fontsize', '42');
      await input('Style Name', 'Heading');
      await review();
      const reviewText = await browser.execute(
        () =>
          document.querySelector('[aria-label="Style change review"]')
            .textContent,
      );
      assert.match(
        reviewText,
        /1\s+cue references and 1\s+Comment events change/,
      );
      await browser.execute(() =>
        document
          .querySelector('[aria-label="Style change review"]')
          .scrollIntoView(),
      );
      await browser.saveScreenshot(
        join(evidence, `${extension}-style-review.png`),
      );
      const changed = await accept();
      assert.equal(changed.cues[0].style, 'Heading');
      assert.equal(changed.cues[0].text, '{\\rHeading}Red title');
      assert.equal(style(changed, 'Heading', 'Fontsize'), '42');
      const native = await preview(`${extension}-style-accepted`);
      await click('Undo edit');
      const undone = await documentIs(
        'atomic style Undo',
        (d) => d.cues[0].style === 'Title',
      );
      await ready();
      assert.deepEqual(undone.cues, opened.cues);
      assert.deepEqual(undone.format, opened.format);
      await click('Redo edit');
      await documentIs(
        'atomic style Redo',
        (d) => d.cues[0].style === 'Heading',
      );
      await ready();
      await menu('saveAs');
      await until('failed style save', () =>
        browser.execute(() =>
          /Cannot|cannot|No such/.test(
            document.querySelector('footer [role="status"]')?.textContent ?? '',
          ),
        ),
      );
      await ready();
      assert.deepEqual((await status()).format, changed.format);
      assert.equal(await readFile(source, 'utf8'), original);
      await menu('saveAs');
      await documentIs(
        'style saved',
        (d) => !d.dirty && samePath(d.path, copy),
      );
      await ready();
      const expected = original
        .split('\r\n')
        .map((line) =>
          line.startsWith('Style: Title,')
            ? line.replace('Title,sans-serif,36', 'Heading,sans-serif,42')
            : /^(Dialogue|Comment):/.test(line)
              ? line
                  .replace('Title,', 'Heading,')
                  .replaceAll('\\rTitle', '\\rHeading')
              : line,
        )
        .join('\r\n');
      assert.equal(await readFile(copy, 'utf8'), expected);
      assert.equal(await readFile(source, 'utf8'), original);
      await menu('saveProject');
      await until('style project saved', async () => {
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
      await documentIs('closed style copy', (d) => !d.loaded);
      await ready();
      await menu('openSubtitles');
      const reopened = await documentIs('style reopened', (d) =>
        samePath(d.path, copy),
      );
      await ready();
      assert.deepEqual(reopened.format, changed.format);
      assert.deepEqual(reopened.cues, changed.cues);
      await menu('closeSubtitles');
      await documentIs('closed for project', (d) => !d.loaded);
      await ready();
      await menu('openProject');
      const restored = await documentIs('style project restored', (d) =>
        samePath(d.path, copy),
      );
      await ready();
      assert.deepEqual(restored.format, changed.format);
      await openStyle('Heading');
      await input('Style Fontsize', '50');
      await review();
      await accept();
      const recoveryBytes = expected.replace(
        'Style: Heading,sans-serif,42',
        'Style: Heading,sans-serif,50',
      );
      await until('style source-property checkpoint', async () => {
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
      await writeFile(copy, expected + '; External newer edit\r\n');
      browser = await restart([
        { kind: 'saveSubtitles', path: copy },
        { kind: 'saveSubtitles', path: recovered },
      ]);
      const row = await browser.$(
        `//div[contains(@class,"recovery-row")][span[starts-with(normalize-space(.),"style-copy.${extension} ")]]`,
      );
      await until('style recovery entry', () => row.isExisting());
      await (await row.$('button=Recover')).click();
      const recovery = await documentIs(
        'source properties recovered',
        (d) =>
          d.requiresSaveAs &&
          d.format?.styles.some((s) => s.name === 'Heading'),
      );
      assert.equal(style(recovery, 'Heading', 'Fontsize'), '50');
      assert.equal(recovery.canUndo, false);
      await ready();
      await menu('saveAs');
      await until('style external conflict', () =>
        browser.execute(() =>
          document
            .querySelector('footer [role="status"]')
            ?.textContent.includes('changed on disk'),
        ),
      );
      await ready();
      assert.equal(
        await readFile(copy, 'utf8'),
        expected + '; External newer edit\r\n',
      );
      await menu('saveAs');
      await documentIs(
        'separate recovered style saved',
        (d) => !d.dirty && samePath(d.path, recovered),
      );
      await ready();
      assert.equal(await readFile(recovered, 'utf8'), recoveryBytes);
      assert.equal(await readFile(source, 'utf8'), original);
      report.cases.push({
        extension,
        sourceSha256: createHash('sha256').update(original).digest('hex'),
        exactBytes: Buffer.byteLength(expected),
        native,
        createCopy: true,
        invalidRejected: true,
        reviewedRename: true,
        cueAndCommentReferences: true,
        undoRedo: true,
        failedSavePreserved: true,
        exactSaveReopen: true,
        projectRestore: true,
        propertyOnlyCrashRecovery: true,
        externalConflictPreserved: true,
      });
      await save();
      await record({
        step: `actual ${extension} style authoring, references, save, history, preview, project and recovery`,
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
