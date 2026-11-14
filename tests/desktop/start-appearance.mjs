import assert from 'node:assert/strict';
import { join } from 'node:path';

async function settledPaint(browser) {
  await browser.execute(async () => {
    await Promise.all(
      document
        .getAnimations()
        .filter((animation) => animation instanceof CSSTransition)
        .map((animation) => animation.finished.catch(() => {})),
    );
    await new Promise((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(resolve)),
    );
  });
}
async function geometry(browser) {
  return browser.execute(async () => {
    const toolbar = document.querySelector('.toolbar');
    const video = document
      .querySelector('.video-space')
      .getBoundingClientRect();
    const drag = document
      .querySelector('[data-window-drag]')
      ?.getBoundingClientRect();
    return {
      platform: navigator.platform,
      decorated: await window.__TAURI__.window.getCurrentWindow().isDecorated(),
      theme: document.documentElement.dataset.theme,
      density: document.documentElement.dataset.density,
      body: getComputedStyle(document.body).backgroundColor,
      toolbar: {
        height: toolbar.getBoundingClientRect().height,
        width: toolbar.clientWidth,
        scrollWidth: toolbar.scrollWidth,
      },
      menus: toolbar.querySelectorAll('nav button').length,
      captions: toolbar.querySelectorAll('.captions button').length,
      drag: drag ? { x: drag.x, right: drag.right } : null,
      video: {
        x: video.x,
        y: video.y,
        width: video.width,
        height: video.height,
      },
      width: innerWidth,
      height: innerHeight,
    };
  });
}
export async function startChrome({
  browser,
  until,
  evidence,
  record,
  restart = false,
}) {
  await until('start screen', () => browser.$('.start-screen').isExisting());
  const state = await geometry(browser);
  assert.equal(state.body, 'rgba(0, 0, 0, 0)');
  assert.equal(state.video.width, 0);
  assert.equal(state.video.height, 0);
  assert.ok(
    state.toolbar.scrollWidth <= state.toolbar.width + 1,
    JSON.stringify(state),
  );
  if (state.platform.startsWith('Mac')) {
    assert.equal(state.decorated, true);
    assert.ok(state.drag?.x >= 88, JSON.stringify(state));
  } else if (state.platform.startsWith('Win')) {
    assert.equal(state.decorated, true);
    assert.equal(state.captions, 0);
    assert.equal(state.menus, 8);
  } else if (state.platform.startsWith('Linux')) {
    assert.equal(state.decorated, !restart);
    assert.equal(state.captions, restart ? 3 : 0);
    assert.equal(state.menus, 8);
  }
  const dropFits = await browser.execute(() => {
    const drop = document.querySelector('.drop').getBoundingClientRect();
    return [...document.querySelectorAll('.drop > *')].every((item) => {
      const rect = item.getBoundingClientRect();
      return rect.left >= drop.left && rect.right <= drop.right;
    });
  });
  assert.ok(dropFits, 'Drop-target text remains inside its border');
  if (restart) {
    assert.equal(state.theme, 'light');
    assert.equal(state.density, 'compact');
    await until('persisted recent work shown on start', () =>
      browser.$('.recent li:first-child').isExisting(),
    );
  }
  if (state.menus) {
    await browser.execute(() =>
      document.querySelector('.toolbar nav button').click(),
    );
    await until('File group menu', () =>
      browser.$('dialog.group-menu[open]').isExisting(),
    );
    assert.equal(await browser.$('dialog.group-menu h2').getText(), 'File');
    assert.equal(
      await browser.$('dialog.group-menu [data-action="undo"]').isExisting(),
      false,
    );
    await browser.execute(() =>
      document
        .querySelector('dialog.group-menu')
        .dispatchEvent(new Event('cancel', { cancelable: true })),
    );
    await until(
      'File menu closed',
      async () => !(await browser.$('dialog.group-menu').isExisting()),
    );
  }
  await settledPaint(browser);
  await browser.saveScreenshot(
    join(
      evidence,
      restart ? 'start-light-restarted.png' : 'start-dark-initial.png',
    ),
  );
  await record({
    step: restart
      ? 'appearance/recent work and platform chrome after process restart'
      : 'start screen and native platform chrome',
    ...state,
    physicalDragOrSnapVerified: false,
  });
}
export async function saveAppearance({
  browser,
  menu,
  invoke,
  until,
  evidence,
  record,
}) {
  await menu('settings');
  await until('Appearance settings loaded', () =>
    browser.$('[aria-label="Theme"]').isEnabled(),
  );
  await browser.$('button=Appearance').click();
  async function select(label, value) {
    await browser.execute(
      (label, value) => {
        const select = document.querySelector(`[aria-label="${label}"]`);
        select.value = value;
        select.dispatchEvent(new Event('change', { bubbles: true }));
      },
      label,
      value,
    );
  }
  await select('Theme', 'light');
  await until(
    'saved light appearance',
    async () => (await invoke('preferences_get')).appearance.theme === 'light',
  );
  await select('Control density', 'comfortable');
  await until(
    'saved comfortable density',
    async () =>
      (await invoke('preferences_get')).appearance.density === 'comfortable',
  );
  await browser.$('button=Close').click();
  await until(
    'appearance closed',
    async () => !(await browser.$('dialog[open]').isExisting()),
  );
  const comfortable = await geometry(browser);
  assert.equal(comfortable.theme, 'light');
  assert.equal(comfortable.density, 'comfortable');
  assert.equal(comfortable.body, 'rgba(0, 0, 0, 0)');
  assert.ok(
    comfortable.video.y >= comfortable.toolbar.height &&
      comfortable.video.x >= 0 &&
      comfortable.video.x + comfortable.video.width <= comfortable.width + 1,
    JSON.stringify(comfortable),
  );
  await settledPaint(browser);
  await browser.saveScreenshot(
    join(evidence, 'workspace-light-comfortable.png'),
  );
  await menu('settings');
  await browser.$('button=Appearance').click();
  await until('density ready', () =>
    browser.$('[aria-label="Control density"]').isEnabled(),
  );
  await select('Control density', 'compact');
  await until(
    'saved compact density',
    async () =>
      (await invoke('preferences_get')).appearance.density === 'compact',
  );
  if (comfortable.platform.startsWith('Linux')) {
    await browser.execute(() => {
      const label = [...document.querySelectorAll('label')].find((label) =>
        label.textContent.includes('Custom title bar'),
      );
      const checkbox = label.querySelector('input');
      checkbox.click();
    });
    await until(
      'saved custom chrome opt-in',
      async () => (await invoke('preferences_get')).appearance.customTitleBar,
    );
  }
  await browser.$('button=Close').click();
  await until(
    'appearance ready for restart',
    async () => !(await browser.$('dialog[open]').isExisting()),
  );
  await record({
    step: 'confirmed light/comfortable native geometry; light/compact saved for restart',
    comfortable,
    preferences: (await invoke('preferences_get')).appearance,
  });
}
