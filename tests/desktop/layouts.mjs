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
  const unavailableSizes = [];
  const media = await invoke('media_action', {
    action: { kind: 'status' },
    session: 0,
  });
  for (const [width, height] of [
    [1100, 720],
    [1440, 900],
  ]) {
    await browser.setWindowSize(width, height);
    const actualWindow = await browser.getWindowSize();
    const clamped = actualWindow.height < height;
    if (clamped) {
      unavailableSizes.push({ requested: { width, height }, actualWindow });
      console.warn(
        `Host clamps requested ${width}x${height} window to ${actualWindow.width}x${actualWindow.height}; larger-size acceptance remains unverified on this host.`,
      );
    }
    // Verify the size the host actually supplied. A small hosted display can
    // clamp macOS windows; the receipt must not claim a 900 px desktop trial.
    // Full-size acceptance also runs locally and in exact-size browser cases.
    const requiredRows = !clamped && height === 900 ? 14 : 8;
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
          rowHeight: document.querySelector('.cue-row').getBoundingClientRect()
            .height,
          list: rect('.cue-list'),
          cueControls: rect('.cue-followbar'),
          video: rect('.video-space'),
          waveform: rect('[aria-label="Waveform"]'),
          minimap: rect('[aria-label="Timeline minimap"]'),
          scrubber: rect('[aria-label="Video scrubber"]'),
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
        bounds.height >= 656,
        'Host cannot supply the minimum tested client area',
      );
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
      assert.ok(
        bounds.scrubber.y >= bounds.video.bottom - 1 &&
          bounds.scrubber.right <= bounds.video.right + 1 &&
          bounds.scrubber.bottom <= bounds.transport.y + 1,
        'scrubber must remain outside native surface: ' +
          JSON.stringify(bounds),
      );
      assert.ok(
        bounds.minimap.y >= bounds.waveform.y &&
          bounds.minimap.bottom <= bounds.waveform.bottom &&
          bounds.minimap.height === 20,
        'minimap stays inside waveform panel: ' + JSON.stringify(bounds),
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
          bounds.list.height >= requiredRows * bounds.rowHeight,
          'Timing row capacity: ' + JSON.stringify(bounds),
        );
      const filename = `layout-${id}-${width}x${height}.png`;
      await browser.saveScreenshot(join(evidence, filename));
      screenshots.push(filename);
      measurements.push({
        preset: id,
        requestedWindow: { width, height },
        actualWindow,
        requiredRows: id === 'timing' ? requiredRows : null,
        ...bounds,
      });
    }
    await chooseCommand(browser, 'layoutTiming');
    await chooseCommand(browser, 'arrangePanels');
    await until('arrange dialog open', () =>
      browser.execute(
        () =>
          !!document.querySelector('dialog[aria-label="Arrange panels"][open]'),
      ),
    );
    await browser.execute(() =>
      document
        .querySelector('button[aria-label="Place Waveform at Video"]')
        .click(),
    );
    await until('waveform/video swapped', () =>
      browser.execute(() => {
        const video = document
          .querySelector('.video-space')
          .getBoundingClientRect();
        const wave = document
          .querySelector('[aria-label="Waveform"]')
          .getBoundingClientRect();
        return (
          video.width > 0 &&
          video.x < wave.x &&
          ![...document.querySelectorAll('button')].find(
            (button) => button.textContent.trim() === 'Done',
          ).disabled
        );
      }),
    );
    const arranged = await browser.execute(() => {
      const bounds = (selector) => {
        const r = document.querySelector(selector).getBoundingClientRect();
        return { x: r.x, y: r.y, width: r.width, height: r.height };
      };
      return {
        video: bounds('.video-space'),
        waveform: bounds('[aria-label="Waveform"]'),
        modal: !!document.querySelector('dialog[open]'),
      };
    });
    const arrangeScreenshot = `layout-arrange-${width}x${height}.png`;
    await browser.saveScreenshot(join(evidence, arrangeScreenshot));
    screenshots.push(arrangeScreenshot);
    await browser.execute(() =>
      [...document.querySelectorAll('button')]
        .find((button) => button.textContent.trim() === 'Done')
        .click(),
    );
    await until('arrange closed', () =>
      browser.execute(
        () =>
          !document.querySelector('dialog[open]') &&
          document.querySelector('main').getAttribute('aria-busy') === 'false',
      ),
    );
    const restored = await browser.execute(() => {
      const r = document.querySelector('.video-space').getBoundingClientRect();
      return { x: r.x, y: r.y, width: r.width, height: r.height };
    });
    assert.deepEqual(
      restored,
      arranged.video,
      'Closing arrange preserves final placeholder bounds',
    );
    measurements.push({
      preset: 'custom-swap',
      requestedWindow: { width, height },
      actualWindow,
      arranged,
      restored,
    });
  }
  await chooseCommand(browser, 'layoutTiming');
  await browser.setWindowSize(1100, 720);
  await record({
    step: 'five workspace presets at requested desktop sizes',
    unavailableSizes,
    screenshots,
    measurements,
    nativePlaybackAvailable: media.available,
    nativeAcknowledgment:
      'modeled browser harness; playback-free desktop checks placeholder bounds',
  });
}
