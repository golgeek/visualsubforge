import { test } from 'node:test';
import assert from 'node:assert/strict';
import { waveformCursor } from './waveform-hit.ts';

test('cursor follows drag, edge, overlapping cue selection and exact range', () => {
  const cues = [
    { id: 1, startMs: 1000, endMs: 3000, text: 'one' },
    { id: 2, startMs: 1500, endMs: 3500, text: 'two' },
  ];
  function at(
    x: number,
    {
      y = 30,
      enabled = true,
      drag = null,
      selected = null,
      range = null,
    }: {
      y?: number;
      enabled?: boolean;
      drag?: 'edge' | 'range' | null;
      selected?: number | null;
      range?: [number, number] | null;
    } = {},
  ) {
    return waveformCursor(
      { x, y },
      { start: 0, span: 5, width: 500 },
      cues,
      selected,
      range,
      { enabled, drag, canResize: true, canSelect: true },
    );
  }
  assert.equal(at(100, { enabled: false, drag: 'edge' }), 'default');
  assert.equal(at(400, { drag: 'edge' }), 'ew-resize');
  assert.equal(at(100, { drag: 'range' }), 'crosshair');
  assert.equal(at(106), 'ew-resize');
  assert.equal(at(107), 'pointer');
  assert.equal(at(200, { selected: 1, range: [1, 3] }), 'crosshair');
  assert.equal(at(200, { selected: 1, range: [1, 2] }), 'pointer');
  assert.equal(at(200, { y: 19 }), 'crosshair');
  assert.equal(at(400), 'crosshair');
});
