import assert from 'node:assert/strict';
import { join } from 'node:path';
import { chooseCommand } from './commands.mjs';

export async function workspaceLayouts({
  browser,
  invoke,
  until,
  evidence,
  record,
}) {
  const screenshots = [];
  const measurements = [];
  const media = await invoke('media_action', {
    action: { kind: 'status' },
    session: 0,
  });
  for (const [width, height] of [
    [1100, 720],
    [1440, 900],
  ]) {
    await browser.setWindowSize(width, height);
    for (const [id, action] of [
      ['timing', 'layoutTiming'],
      ['editorAbove', 'layoutEditorAbove'],
      ['review', 'layoutReview'],
      ['translation', 'layoutTranslation'],
      ['bigPicture', 'layoutBigPicture'],
    ]) {
      await chooseCommand(browser, action);
      await until('settled layout ' + id, () =>
        browser.execute(
          (id) =>
            document.querySelector('.workspace')?.dataset.preset === id &&
            document.querySelector('main')?.getAttribute('aria-busy') ===
              'false',
          id,
        ),
      );
      // ResizeObserver measurements settle in rendering frames, not a timer.
      await browser.execute(
        () =>
          new Promise((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(resolve)),
          ),
      );
      const bounds = await browser.execute(() => {
        const rect = (selector) => {
          const r = document.querySelector(selector).getBoundingClientRect();
          return {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
            right: r.right,
            bottom: r.bottom,
          };
        };
        const editor = document.querySelector('[aria-label="Subtitle text"]');
        const css = getComputedStyle(editor);
        return {
          width: innerWidth,
          height: innerHeight,
          list: rect('.cue-list'),
          cueControls: rect('.cue-followbar'),
          video: rect('.video-space'),
          waveform: rect('[aria-label="Waveform"]'),
          transport: rect('.transport'),
          editor: rect('[aria-label="Subtitle text"]'),
          textLines:
            (editor.clientHeight -
              parseFloat(css.paddingTop) -
              parseFloat(css.paddingBottom)) /
            parseFloat(css.lineHeight),
        };
      });
      assert.ok(
        bounds.video.x >= 0 &&
          bounds.video.right <= bounds.width &&
          bounds.video.bottom <= bounds.transport.y + 1,
        JSON.stringify(bounds),
      );
      assert.ok(
        bounds.video.x >= bounds.waveform.right,
        'video overlaps waveform',
      );
      assert.ok(bounds.list.height >= 64, 'preset loses usable cue rows');
      assert.ok(
        bounds.cueControls.right <= bounds.list.right + 1,
        'cue header controls clipped: ' + JSON.stringify(bounds),
      );
      assert.ok(
        bounds.textLines >= 2,
        'editor loses two text lines: ' + JSON.stringify(bounds),
      );
      if (id === 'timing')
        assert.ok(
          bounds.list.height >= (height === 720 ? 8 : 14) * 32,
          'Timing row capacity: ' + JSON.stringify(bounds),
        );
      const filename = `layout-${id}-${width}x${height}.png`;
      await browser.saveScreenshot(join(evidence, filename));
      screenshots.push(filename);
      measurements.push({ preset: id, window: { width, height }, ...bounds });
    }
  }
  await chooseCommand(browser, 'layoutTiming');
  await browser.setWindowSize(1100, 720);
  await record({
    step: 'five workspace presets at both desktop sizes',
    screenshots,
    measurements,
    nativePlaybackAvailable: media.available,
    nativeAcknowledgment:
      'modeled browser harness; playback-free desktop checks placeholder bounds',
  });
}
