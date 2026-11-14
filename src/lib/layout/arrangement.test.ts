import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  arrangementGeometry,
  movePanel,
  presetArrangement,
  refusal,
} from './arrangement.ts';
const space = { width: 1084, height: 620, editor: 210, transport: 38 };
const base = () => presetArrangement('timing', 65, false, false);
test('swaps preserve unique panels, row proportions and the input snapshot', () => {
  const original = base();
  const next = movePanel(original, {
    panel: 'waveform',
    target: 'video',
    kind: 'swap',
  });
  assert.deepEqual(next.rows[0]!.panels, ['video', 'waveform']);
  assert.deepEqual(original.rows[0]!.panels, ['waveform', 'video']);
  assert.equal(next.rows[0]!.split, 65);
  assert.equal(new Set(next.rows.flatMap((row) => row.panels)).size, 5);
});
test('moves remove empty source rows and hidden panels can replace a visible slot', () => {
  let value = movePanel(base(), {
    panel: 'editor',
    target: 'cues',
    kind: 'before',
  });
  assert.deepEqual(
    value.rows.map((row) => row.panels),
    [['waveform', 'video'], ['transport'], ['editor'], ['cues']],
  );
  value = movePanel(value, { panel: 'video', kind: 'hide' });
  assert.deepEqual(value.rows[0]!.panels, ['waveform']);
  value = movePanel(value, {
    panel: 'video',
    target: 'waveform',
    kind: 'swap',
  });
  assert.deepEqual(value.rows[0]!.panels, ['video']);
  assert.equal(
    value.rows.flatMap((row) => row.panels).includes('waveform'),
    false,
  );
});
test('insertion and refusal preserve panel minima at small and large sizes', () => {
  assert.equal(refusal(base(), space), null);
  const tall = movePanel(base(), {
    panel: 'video',
    target: 'waveform',
    kind: 'after',
  });
  assert.match(refusal(tall, { ...space, height: 600 })!, /height/);
  const narrow = movePanel(base(), {
    panel: 'editor',
    target: 'cues',
    kind: 'beside',
  });
  assert.match(refusal(narrow, space)!, /width/);
  assert.equal(refusal(narrow, { ...space, width: 1424 }), null);
  const geometry = arrangementGeometry(base(), { ...space, height: 100 });
  assert.ok(
    geometry.height > 100,
    'resize uses minimum overflow, never squashes controls',
  );
  assert.ok(geometry.rows[3]! >= space.editor);
  assert.throws(
    () =>
      movePanel(base(), {
        panel: 'editor',
        target: 'waveform',
        kind: 'beside',
      }),
    /at most two/,
  );
  assert.throws(
    () =>
      movePanel(
        { version: 1, rows: [{ panels: ['video'], weight: 1, split: 50 }] },
        { panel: 'video', kind: 'hide' },
      ),
    /at least one/,
  );
});
