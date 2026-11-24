import { expect, type Locator, type Page } from '@playwright/test';

// Keep sampling through the modeled reply, including the first paint after it.
export async function expectStableUnderPointer(
  page: Page,
  target: Locator,
  action: () => Promise<void>,
  settled: () => Promise<void>,
) {
  await target.scrollIntoViewIfNeeded();
  const box = await target.boundingBox();
  if (!box) throw new Error('Pointer target has no box');
  const point = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  await page.mouse.move(point.x, point.y);
  const observation = await target.evaluateHandle((el, point) => {
    const initial = el.getBoundingClientRect();
    const hit = document.elementFromPoint(point.x, point.y);
    if (!hit || !el.contains(hit))
      throw new Error('Pointer misses target before action');
    let samples = 0,
      moved = false;
    let frame = 0;
    function sample() {
      const current = el.getBoundingClientRect();
      samples++;
      moved ||=
        document.elementFromPoint(point.x, point.y) !== hit ||
        (['x', 'y', 'width', 'height'] as const).some(
          (k) => Math.abs(current[k] - initial[k]) > 1,
        );
    }
    function nextFrame() {
      sample();
      frame = requestAnimationFrame(nextFrame);
    }
    nextFrame();
    return () => {
      sample();
      cancelAnimationFrame(frame);
      return { samples, moved };
    };
  }, point);
  let result;
  try {
    await action();
    await settled();
  } finally {
    result = await observation.evaluate((f) => f());
    await observation.dispose();
  }
  expect(result.samples).toBeGreaterThanOrEqual(3);
  expect(result.moved).toBe(false);
}
