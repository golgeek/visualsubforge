// Start at the editor commit and stop at publication of the new index. Both
// timestamps come from the WebView, independently of WebDriver polling.
import assert from 'node:assert/strict';
import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { prepareSceneFixtures } from './scene-quality.mjs';

export async function qualityIndexBudget({
  lane,
  root,
  menu,
  status,
  invoke,
  until,
  record,
  restart,
}) {
  const scenes = lane === 'scenes';
  for (const english of [false, true]) {
    const budgetMs = 100;
    const findings = english ? 50000 : 20000;
    const data = join(root, `index-${lane}-${english ? 'english' : 'off'}`);
    await mkdir(data);
    const path = join(
      data,
      scenes ? 'scene-large.vsfproject' : 'quality-10000.srt',
    );
    const browser = await restart(
      [{ kind: scenes ? 'project' : 'subtitles', path }],
      data,
    );
    const { current } = await invoke('quality_settings');
    const tools = {
      dictionary: 'en-US',
      spelling: true,
      typography: true,
      capitalization: true,
      dialogueMarkers: true,
    };
    if (scenes) {
      await prepareSceneFixtures(data, current);
      if (english) {
        const project = JSON.parse(await readFile(path, 'utf8'));
        project.options.quality.languageTools = tools;
        await writeFile(path, JSON.stringify(project));
      }
    } else {
      await copyFile(join(root, 'quality-10000.srt'), path);
      if (english) {
        const preferences = await invoke('preferences_get');
        preferences.quality = {
          ...current.profile,
          id: 'budget-en-us',
          name: 'Budget English',
          languageTools: tools,
        };
        await invoke('preferences_save', { preferences, epoch: current.epoch });
      }
    }
    await menu(scenes ? 'openProject' : 'openSubtitles');
    const doc = await until('original 10,000-cue fixture', async () => {
      const doc = await status();
      return doc.cues.length === 10000 && doc;
    });
    const waitForRevision = (revision) =>
      until('complete index for the exact accepted revision', async () =>
        browser.execute(
          (generation, revision, findings) => {
            const mark = performance
              .getEntriesByName('quality-index-complete')
              .at(-1);
            return (
              mark?.detail.generation === generation &&
              mark.detail.revision === revision &&
              mark.detail.cues === 10000 &&
              mark.detail.findings === findings
            );
          },
          doc.generation,
          revision,
          findings,
        ),
      );
    let revision = doc.revision;
    await waitForRevision(revision);
    const samples = [];
    for (let i = 0; i < 20; ++i) {
      const began = await browser.execute(async (text) => {
        const editor = document.querySelector('[aria-label="Subtitle text"]');
        if (editor.value !== text)
          throw new Error('Expected original cue text before timed commit');
        editor.focus();
        editor.value += '!';
        editor.dispatchEvent(new Event('input', { bubbles: true }));
        await new Promise((resolve) => requestAnimationFrame(resolve));
        const start = performance.now();
        await window.__vsfEditorInput.commit();
        return start;
      }, doc.cues[0].text);
      await waitForRevision(++revision);
      samples.push(
        await browser.execute(
          (start) =>
            performance.getEntriesByName('quality-index-complete').at(-1)
              .startTime - start,
          began,
        ),
      );
      await (await browser.$('button=Undo')).click();
      await waitForRevision(++revision);
    }
    assert.deepEqual((await status()).cues, doc.cues);
    const p95Ms = [...samples].sort((a, b) => a - b)[
      Math.ceil(samples.length * 0.95) - 1
    ];
    await record({
      step: 'commit to complete 10,000-cue issue index',
      lane,
      english,
      findings,
      budgetMs,
      samples,
      p95Ms,
    });
    assert.ok(
      p95Ms <= budgetMs,
      `${lane} (${english ? 'English' : 'language off'}) commit to index p95 ${p95Ms} ms exceeds ${budgetMs} ms`,
    );
  }
}
