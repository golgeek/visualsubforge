// Real WebView/Rust/helper/files and native preview. Chooser results and DOM
// input are supplied. OS picker, physical input and full-scale gates are separate.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { observePluginChildren } from './plugin-processes.mjs';
const run = promisify(execFile);
const hash = (value) => createHash('sha256').update(value).digest('hex');

export async function pluginWorkflow({
  restart,
  root,
  evidence,
  menu,
  invoke,
  status,
  until,
  record,
  binary,
  getPid,
  samePath,
  withNativePreview = true,
}) {
  const runner = join(
    dirname(binary),
    `vsf${process.platform === 'win32' ? '.exe' : ''}`,
  );
  const report = {
    status: 'running',
    withNativePreview,
    binary,
    binarySha256: hash(await readFile(binary)),
    runner,
    runnerSha256: hash(await readFile(runner)),
    scope:
      'Real native WebView, Rust owner and installed sibling CLI helper, four formats and both APIs; exact bytes, history, failed saves and recovery. Supplied file choices/DOM events; physical GPU/audio and OS picker acceptance remain separate.',
    cases: [],
    controls: [],
    captures: [],
  };
  const save = () =>
    writeFile(
      join(
        evidence,
        withNativePreview ? 'plugins.json' : 'plugins-editor.json',
      ),
      JSON.stringify(report, null, 2) + '\n',
    );
  const video = join(root, 'plugin-video.mkv');
  if (withNativePreview)
    await run(
      process.env.VSF_FFMPEG ?? 'ffmpeg',
      [
        '-v',
        'error',
        '-nostdin',
        '-f',
        'lavfi',
        '-i',
        'color=c=black:s=640x360:r=24:d=10',
        '-c:v',
        'libx264',
        '-preset',
        'ultrafast',
        '-threads',
        '1',
        video,
      ],
      { timeout: 30000 },
    );
  let browser;
  const media = () =>
    invoke('media_action', { action: { kind: 'status' }, session: 0 });
  const documentIs = (label, predicate) =>
    until(label, async () => {
      const d = await status();
      return predicate(d) && d;
    });
  const ready = async () => {
    await browser.execute(
      () =>
        new Promise((resolve) =>
          requestAnimationFrame(() => requestAnimationFrame(resolve)),
        ),
    );
    return until('plugin workspace ready', () =>
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
        const b = [...document.querySelectorAll('button')].find(
          (b) => b.textContent.trim() === label && !b.disabled,
        );
        if (!b) return false;
        b.click();
        return true;
      }, label),
    );
  const input = (label, value) =>
    browser.execute(
      (label, value) => {
        const e = document.querySelector(`[aria-label="${label}"]`);
        if (!e || e.disabled) throw new Error(`Input unavailable: ${label}`);
        e.value = value;
        e.dispatchEvent(new Event('input', { bubbles: true }));
      },
      label,
      value,
    );
  const dialogText = () =>
    browser.execute(
      () =>
        document.querySelector('#plugin-title')?.closest('dialog')
          ?.textContent ?? '',
    );
  const footer = () =>
    browser.$('footer [role="status"]').then((e) => e.getText());
  const open = async () => {
    await ready();
    await menu('plugins');
    await until('plugin dialog', () => browser.$('#plugin-title').isExisting());
    // Explicit test preference; profile portability is covered by owner tests.
    await browser.execute(() => {
      document.querySelector('fieldset details').open = true;
    });
    await input('Plugin CPS target', '20');
  };
  const preview = async () => {
    await click('Preview plugin fixes');
    await until('plugin fix review', async () => {
      const text = await dialogText();
      assert.ok(!text.includes('Plugin: failed'), text);
      return (
        (await browser.$('button=Accept plugin changes').isExisting()) &&
        (await browser.$('button=Accept plugin changes').isEnabled())
      );
    });
  };
  const nativePreview = async () => {
    const d = await status();
    await invoke('media_action', {
      action: { kind: 'seek', seconds: d.cues[0].startMs / 1000 + 0.2 },
      session: (await media()).session,
    });
    return until('accepted native plugin preview', async () => {
      const m = await media();
      assert.equal(m.error, null);
      assert.equal(m.subtitlePreviewError, null);
      return (
        m.loaded &&
        m.paused &&
        !m.seeking &&
        m.videoOutputConfigured &&
        m.renderedFrames > 0 &&
        m.previewDocumentGeneration === d.generation &&
        m.previewDocumentRevision === d.revision &&
        m
      );
    });
  };
  const capture = async (label) => {
    await browser.saveScreenshot(join(evidence, `${label}-webview.png`));
    const item = { label, document: await status(), media: await media() };
    report.captures.push(item);
    await save();
    if (
      process.env.VSF_PLUGIN_CAPTURE === '1' &&
      (!process.env.VSF_PLUGIN_CAPTURE_LABELS ||
        process.env.VSF_PLUGIN_CAPTURE_LABELS.split(',').includes(label))
    ) {
      await writeFile(
        join(evidence, 'capture-request.json'),
        JSON.stringify(item, null, 2),
      );
      console.log(
        `Native plugin capture ready: ${label}; evidence: ${evidence}`,
      );
      await until(
        `native observation ${label}`,
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
  };
  try {
    for (const api of ['vsf-1', 'vss-1'])
      for (const format of ['srt', 'ass', 'ssa', 'vtt']) {
        const label = `${api}-${format}`,
          source = join(root, `原文 ${label}.${format}`),
          output = join(root, `accepted-${label}.${format}`),
          recovered = join(root, `recovered-${label}.${format}`);
        let original =
          format === 'srt'
            ? '\uFEFF7\r\n00:00:01,000 --> 00:00:03,000\r\n{\\an8}{\\i1}Été 世界{\\i0}\r\n\r\n9\r\n00:00:03,120 --> 00:00:04,000\r\nSecond <i>😀</i>\r\n\r\n'
            : await readFile(
                resolve(`tests/fixtures/subtitles/styles.${format}`),
                'utf8',
              );
        if (format === 'ass' || format === 'ssa')
          original = original.replace('Red title\n', '{\\i1}Red title{\\i0}\n');
        await writeFile(source, original, { flag: 'wx' });
        const directory = join(root, `plugin 包 ${label}`),
          manifest = join(directory, 'manifest.json'),
          script = join(directory, 'main.js');
        await mkdir(directory);
        const setting = api === 'vsf-1' ? 'suffix' : 'ParamSuffix';
        const definition = {
          version: 1,
          id: `test.${label}`,
          name: `Native append ${label}`,
          api,
          entry: 'main.js',
          requires: ['rules', 'fixes'],
          settings: { [setting]: '!' },
        };
        const code =
          api === 'vsf-1'
            ? `globalThis.VSFPlugin={check(c,h){return c.text.endsWith(h.settings.suffix)?'':'Append marker';},fix(c,h){h.update(c.id,{text:c.text+h.settings.suffix});}};`
            : `globalThis.VSSPlugin={Name:'Native VSS append',ParamSuffix:{Value:'!'},HasError(c){return c.Text.endsWith(this.ParamSuffix.Value)?'':'Append marker';},FixError(c){c.Text+=this.ParamSuffix.Value;}};`;
        await writeFile(manifest, JSON.stringify(definition));
        await writeFile(script, code);
        browser = await restart([
          ...(withNativePreview ? [{ kind: 'media', path: video }] : []),
          { kind: 'subtitles', path: source },
          { kind: 'plugin', path: null },
          { kind: 'plugin', path: manifest },
          { kind: 'saveSubtitles', path: null },
          {
            kind: 'saveSubtitles',
            path: join(root, 'missing-parent', `invalid.${format}`),
          },
          { kind: 'saveSubtitles', path: output },
          { kind: 'subtitles', path: output },
          { kind: 'plugin', path: manifest },
        ]);
        if (withNativePreview) {
          await menu('openMedia');
          await until(
            'native plugin media',
            async () => (await media()).loaded,
          );
          await until('completed scene context and paused cursor', async () => {
            const m = await media();
            return (
              m.paused &&
              !m.seeking &&
              m.positionSeconds !== null &&
              (await footer()).includes('0 visual cuts')
            );
          });
        } else {
          assert.equal((await media()).loaded, false);
        }
        await ready();
        await menu('openSubtitles');
        const initial = await documentIs(
          'plugin source',
          (d) => d.loaded && d.cues.length === 2,
        );
        await ready();
        await input('Subtitle text', 'Protected plugin draft');
        await open();
        await click('Choose plugin…');
        await until('cancelled chooser completed', () =>
          browser.$('button=Choose plugin…').isEnabled(),
        );
        assert.equal(await browser.$('button=Run checker').isEnabled(), false);
        await click('Choose plugin…');
        await until('chosen plugin', async () =>
          (await dialogText()).includes(manifest),
        );
        assert.equal(await browser.$('button=Run checker').isEnabled(), false);
        await click('Discard cue draft');
        await click('Run checker');
        await until('plugin checker complete', async () =>
          (await dialogText()).includes('Plugin: reviewing'),
        );
        assert.ok((await dialogText()).includes('Findings (2)'));
        assert.deepEqual(await status(), initial);
        if (api === 'vsf-1' && format === 'srt') {
          // Actual loop helper observed live before UI cancellation. Only this
          // owned app's child PIDs are inspected; no unrelated process is signalled.
          await writeFile(
            script,
            'globalThis.VSFPlugin={check(){while(true){}},fix(){}};',
          );
          const observer = await observePluginChildren(getPid());
          try {
            await click('Run checker');
            const pids = await until('live plugin helper', async () => {
              const p = await observer.sample();
              return p.length && p;
            });
            assert.equal(pids.length, 1);
            await click('Cancel plugin');
            await until('cancelled plugin', async () =>
              (await dialogText()).includes('Plugin: cancelled'),
            );
            await until(
              'plugin helper reaped',
              async () => (await observer.sample()).length === 0,
            );
            report.controls.push({
              name: 'live-loop UI cancellation and reaping',
              pids,
            });
          } finally {
            await observer.close();
          }
          assert.deepEqual(await status(), initial);
          await writeFile(
            script,
            "globalThis.VSFPlugin={check(){return 'control';},fix(){throw new Error('native rollback control');}};",
          );
          await click('Preview plugin fixes');
          await until('exception rollback report', async () =>
            (await dialogText()).includes('native rollback control'),
          );
          assert.deepEqual(await status(), initial);
          await writeFile(script, code);
          await preview();
          await writeFile(script, `${code}\n// changed after review\n`);
          await click('Accept plugin changes');
          await until(
            'changed package refused',
            async () =>
              /changed/i.test(await dialogText()) &&
              (await browser.$('button=Preview plugin fixes').isEnabled()),
          );
          assert.deepEqual(await status(), initial);
          report.controls.push({
            name: 'exception rollback and package revalidation preserve exact state',
          });
        }
        await preview();
        assert.equal(await readFile(source, 'utf8'), original);
        assert.equal(
          (await browser.$$('dialog[open] article')).length,
          initial.cues.length,
        );
        await capture(`plugin-${label}-review`);
        await click('Accept plugin changes');
        const accepted = await documentIs(
          'atomic plugin acceptance',
          (d) =>
            d.revision === initial.revision + 1 &&
            d.cues.every((c) => c.text.endsWith('!')),
        );
        await ready();
        assert.equal(accepted.canUndo, true);
        assert.deepEqual(accepted.format, initial.format);
        assert.deepEqual(
          accepted.cues.map((c) => ({ ...c, text: c.text.slice(0, -1) })),
          initial.cues,
        );
        const native = withNativePreview ? await nativePreview() : null;
        await capture(`plugin-${label}-accepted`);
        await click('Undo edit');
        await documentIs(
          'one plugin Undo',
          (d) => JSON.stringify(d.cues) === JSON.stringify(initial.cues),
        );
        await ready();
        await click('Redo edit');
        await documentIs(
          'one plugin Redo',
          (d) => JSON.stringify(d.cues) === JSON.stringify(accepted.cues),
        );
        await ready();
        const beforeSave = await status();
        await menu('saveAs');
        await ready();
        assert.deepEqual(await status(), beforeSave);
        await menu('saveAs');
        await until('failed plugin save', async () =>
          /directory|No such|cannot/i.test(await footer()),
        );
        await ready();
        assert.deepEqual(await status(), beforeSave);
        let expected = original;
        for (const cue of initial.cues) {
          const raw = cue.text.replaceAll(
            '\n',
            original.includes('\r\n') ? '\r\n' : '\n',
          );
          assert.equal(
            expected.split(raw).length,
            2,
            'Fixture cue text must occur exactly once',
          );
          expected = expected.replace(raw, `${raw}!`);
        }
        await menu('saveAs');
        await documentIs(
          'plugin output saved',
          (d) => samePath(d.path, output) && !d.dirty,
        );
        await ready();
        assert.equal(await readFile(output, 'utf8'), expected);
        assert.equal(await readFile(source, 'utf8'), original);
        await menu('openSubtitles');
        await documentIs(
          'plugin output reopened',
          (d) =>
            samePath(d.path, output) && d.generation !== initial.generation,
        );
        await ready();
        assert.deepEqual((await status()).cues, accepted.cues);
        assert.deepEqual((await status()).format, initial.format);
        definition.settings[setting] = '?';
        await writeFile(manifest, JSON.stringify(definition));
        await open();
        await click('Choose plugin…');
        await until('recovery package selected', async () =>
          (await dialogText()).includes(manifest),
        );
        await preview();
        await click('Accept plugin changes');
        const checkpoint = await documentIs('plugin recovery edit', (d) =>
          d.cues.every((c) => c.text.endsWith('!?')),
        );
        await ready();
        for (const cue of accepted.cues) {
          const raw = cue.text.replaceAll(
            '\n',
            expected.includes('\r\n') ? '\r\n' : '\n',
          );
          expected = expected.replace(raw, `${raw}?`);
        }
        await until('durable plugin checkpoint', async () => {
          for (const entry of await readdir(join(root, 'recovery-v1'), {
            withFileTypes: true,
          })) {
            if (!entry.isDirectory()) continue;
            try {
              const stored = JSON.parse(
                await readFile(
                  join(root, 'recovery-v1', entry.name, 'snapshot.json'),
                  'utf8',
                ),
              );
              if (
                samePath(stored.source_path, output) &&
                stored.generation === checkpoint.generation &&
                stored.revision === checkpoint.revision &&
                stored.accepted_srt === expected
              )
                return true;
            } catch (e) {
              if (e.code !== 'ENOENT') throw e;
            }
          }
          return false;
        });
        browser = await restart([{ kind: 'saveSubtitles', path: recovered }]);
        await until('recover plugin document', () =>
          browser.execute((path) => {
            const row = [...document.querySelectorAll('.recovery-row')].find(
              (r) => r.querySelector('span')?.title === path,
            );
            const b =
              row &&
              [...row.querySelectorAll('button')].find(
                (b) => b.textContent.trim() === 'Recover' && !b.disabled,
              );
            if (!b) return false;
            b.click();
            return true;
          }, checkpoint.path),
        );
        const restored = await documentIs(
          'plugin document recovered',
          (d) => d.requiresSaveAs && d.cues.every((c) => c.text.endsWith('!?')),
        );
        await ready();
        assert.equal(restored.canUndo, false);
        assert.deepEqual(restored.cues, checkpoint.cues);
        assert.deepEqual(restored.format, initial.format);
        await menu('saveAs');
        await documentIs(
          'recovered plugin saved',
          (d) => samePath(d.path, recovered) && !d.dirty,
        );
        await ready();
        assert.equal(await readFile(recovered, 'utf8'), expected);
        assert.equal(await readFile(source, 'utf8'), original);
        report.cases.push({
          api,
          format,
          sourceSha256: hash(original),
          recoveredSha256: hash(expected),
          sourcePreserved: true,
          exactBytes: true,
          oneUndoRedo: true,
          failedSavePreserved: true,
          recovery: true,
          native: native
            ? {
                videoOutputConfigured: native.videoOutputConfigured,
                hardwareDecoder: native.hardwareDecoder,
                renderedFrames: native.renderedFrames,
                generation: native.previewDocumentGeneration,
                revision: native.previewDocumentRevision,
              }
            : null,
        });
        await save();
        await record({
          step: `plugin ${label}: ${withNativePreview ? 'native preview and ' : 'subtitle-only '}check/fix/history/save/reopen/recovery`,
          bytes: Buffer.byteLength(expected),
        });
      }
    report.status = 'passed';
    await save();
  } catch (error) {
    report.status = 'failed';
    report.error = String(error.stack ?? error);
    report.dialog = browser ? await dialogText().catch(() => '') : '';
    await save();
    throw error;
  }
}
