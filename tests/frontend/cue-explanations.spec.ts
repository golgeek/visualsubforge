import { expect, test } from '@playwright/test';

test('the selected cue has distinct endpoint chips and its own explanations', async ({
  page,
}) => {
  await page.goto('/tests/frontend/cue-editor/?explanations');
  await page.locator('[data-cue-id="35"]').click();
  const strip = page.locator('.issue-strip');
  await expect(
    strip.getByRole('button', { name: 'Start near shot change' }),
  ).toBeVisible();
  await expect(
    strip.getByRole('button', { name: 'End near shot change' }),
  ).toBeVisible();
  const launch = strip.getByRole('button', { name: 'All explanations…' });
  await launch.click();
  const dialog = page.getByRole('dialog', { name: 'All cue issues' });
  await expect(dialog.locator('li')).toHaveCount(4);
  await expect(dialog.locator('li')).toHaveText([
    /scene_start detail for cue 35/,
    /scene_end detail for cue 35/,
    /gap detail for cue 35/,
    /line_length detail for cue 35/,
  ]);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(launch).toBeFocused();
  const overflow = strip.getByRole('button', {
    name: 'Show all 4 cue issue groups',
  });
  await overflow.click();
  await expect(dialog.locator('li')).toHaveCount(4);
  await dialog.press('Escape');
  await expect(overflow).toBeFocused();
  await expect(
    strip.getByRole('button', { name: 'Snap to shot change…' }),
  ).toBeVisible();
  await launch.click();
  await dialog.getByRole('button', { name: 'Show all issues in file' }).click();
  await expect(dialog).toHaveCount(0);
  await expect(
    page
      .getByRole('complementary', { name: 'Review panel' })
      .getByRole('button', { name: /^Issues/ }),
  ).toHaveAttribute('aria-pressed', 'true');
});

for (const delay of [0, 120]) {
  test(`typing keeps all issue strip controls stable during a ${delay} ms read`, async ({
    page,
  }) => {
    await page.goto(
      '/tests/frontend/cue-editor/?explanations&readDelay=' + delay,
    );
    await page.locator('[data-cue-id="35"]').click();
    const strip = page.locator('.issue-strip');
    const fix = strip.getByRole('button', { name: 'Fix 1', exact: true });
    await expect(fix).toBeEnabled();
    const editor = page.getByRole('textbox', { name: 'Subtitle text' });
    await editor.focus();
    const snapshot = await strip.evaluateHandle((el) => {
      const editorRoot = el.closest('.editor')!;
      const ancestors: Element[] = [];
      for (
        let parent = el.parentElement;
        parent;
        parent = parent.parentElement
      ) {
        if (
          parent === document.scrollingElement ||
          parent.scrollHeight > parent.clientHeight ||
          /auto|scroll|hidden|overlay/.test(getComputedStyle(parent).overflowY)
        )
          ancestors.push(parent);
      }
      function geometry() {
        const top = el.getBoundingClientRect().top;
        return {
          top,
          relativeTop: top - editorRoot.getBoundingClientRect().top,
          page: document.scrollingElement!.scrollTop,
          ancestors: ancestors.map((parent) => [
            parent.tagName + '.' + parent.className,
            parent.scrollTop,
          ]),
        };
      }

      const sample = () =>
        JSON.stringify({
          geometry: geometry(),
          opacity: getComputedStyle(el).opacity,
          controls: [...el.querySelectorAll('button')].map((b) => [
            b.textContent,
            b.disabled,
            getComputedStyle(b).opacity,
            b.getBoundingClientRect().toJSON(),
          ]),
        });
      const baseline = sample();
      let changed = '';
      let frames = 0;
      let frame = 0;
      function observe() {
        frames++;
        const now = sample();
        if (now !== baseline && !changed) changed = now;
        frame = requestAnimationFrame(observe);
      }
      observe();
      return () => {
        cancelAnimationFrame(frame);
        return { baseline, changed, frames };
      };
    });
    await editor.press('End');
    await editor.pressSequentially('xy', { delay });
    const draft = await editor.inputValue();
    // Avoid a focus change here: the draft and the read must still guard Fix.
    await strip.evaluate((el) =>
      [...el.querySelectorAll('button')]
        .find((b) => b.textContent?.trim() === 'Fix 1')
        ?.click(),
    );
    await expect(page.locator('#test')).toHaveAttribute(
      'data-checked-text',
      draft,
    );
    const observation = await snapshot.evaluate((stop) => stop());
    await snapshot.dispose();
    expect(observation.frames).toBeGreaterThan(2);
    expect(observation.changed || observation.baseline).toBe(
      observation.baseline,
    );
    await expect(editor).toHaveValue(draft);
  });
}
