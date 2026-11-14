import { test } from 'node:test';
import assert from 'node:assert/strict';
import { wheelViewport } from './waveform-navigation.ts';
const input = {
  deltaX: 0,
  deltaY: 0,
  deltaMode: 0,
  shiftKey: false,
  offsetX: 250,
};

test('Horizontal and shift-wheel pans preserve zoom and clamp to media bounds', () => {
  assert.deepEqual(
    wheelViewport(10, 20, 100, 1000, { ...input, deltaX: 100, deltaY: 5 }),
    { start: 12, span: 20 },
  );
  assert.deepEqual(
    wheelViewport(10, 20, 100, 1000, {
      ...input,
      deltaY: -100,
      shiftKey: true,
    }),
    { start: 8, span: 20 },
  );
  assert.deepEqual(
    wheelViewport(0, 20, 100, 1000, { ...input, deltaX: -100 }),
    { start: 0, span: 20 },
  );
  assert.deepEqual(
    wheelViewport(79, 20, 100, 1000, { ...input, deltaX: 100 }),
    { start: 80, span: 20 },
  );
});

test('Vertical wheel zooms around the pointer, and opposite deltas restore the view', () => {
  const zoomed = wheelViewport(10, 20, 100, 1000, { ...input, deltaY: -100 })!;
  assert.ok(zoomed.span < 20);
  assert.ok(Math.abs(zoomed.start + zoomed.span * 0.25 - 15) < 1e-10);
  const restored = wheelViewport(zoomed.start, zoomed.span, 100, 1000, {
    ...input,
    deltaY: 100,
  })!;
  assert.ok(Math.abs(restored.start - 10) < 1e-10);
  assert.ok(Math.abs(restored.span - 20) < 1e-10);
  assert.deepEqual(
    wheelViewport(0, 100, 100, 1000, { ...input, deltaY: 1000 }),
    { start: 0, span: 100 },
  );
  assert.equal(
    wheelViewport(1, 0.1, 100, 1000, { ...input, deltaY: -1000 })!.span,
    0.1,
  );
});

test('Wheel units are normalized; absent media and invalid/empty events are ignored', () => {
  assert.deepEqual(
    wheelViewport(10, 20, 100, 1000, { ...input, deltaX: 1, deltaMode: 1 }),
    { start: 10.32, span: 20 },
  );
  assert.deepEqual(
    wheelViewport(10, 20, 100, 1000, { ...input, deltaX: 1, deltaMode: 2 }),
    { start: 30, span: 20 },
  );
  assert.equal(wheelViewport(0, 20, 0, 1000, { ...input, deltaY: 100 }), null);
  assert.equal(wheelViewport(0, 20, 100, 0, { ...input, deltaY: 100 }), null);
  assert.equal(
    wheelViewport(0, 20, 100, 1000, { ...input, deltaY: NaN }),
    null,
  );
  assert.equal(wheelViewport(0, 20, 100, 1000, input), null);
});

test('Playback follow advances before the old edge, keeps lookahead, and handles wraps/EOF', async () => {
  const { playbackViewportStart } = await import('./waveform-navigation.ts');
  assert.equal(playbackViewportStart(5, 20, 100), 0);
  assert.equal(playbackViewportStart(10, 20, 100), 2);
  assert.ok(Math.abs(playbackViewportStart(10.1, 20, 100) - 2.1) < 1e-10);
  assert.equal(playbackViewportStart(20, 20, 100), 12);
  assert.equal(playbackViewportStart(99, 20, 100), 80);
  assert.equal(playbackViewportStart(2, 20, 100), 0);
  assert.equal(playbackViewportStart(2, 5, 5), 0);
});

test('Peak requests remain stable during scroll and cover lookahead at viewport density', async () => {
  const { waveformRequestWindow } = await import('./waveform-navigation.ts');
  const initial = waveformRequestWindow(0, 20, 100, 600);
  assert.deepEqual(waveformRequestWindow(9.9, 20, 100, 600), initial);
  assert.deepEqual(initial, { start: 0, end: 50, pixels: 1500 });
  for (const start of [0, 10, 19.9, 20, 49, 80]) {
    const request = waveformRequestWindow(start, 20, 100, 600);
    assert.ok(request.start <= start && request.end >= start + 20);
    assert.equal(request.pixels / (request.end - request.start), 30);
  }
  assert.ok(waveformRequestWindow(0, 20, 100, 3000).pixels <= 4096);
});
