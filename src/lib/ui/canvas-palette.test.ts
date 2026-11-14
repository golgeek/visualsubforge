import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { canvasTokens, readCanvasPalette } from './canvas-palette.ts';

const css = readFileSync(
  new URL('./tokens.css', import.meta.url),
  'utf8',
).replace(/\/\*[\s\S]*?\*\//g, '');

/** Custom properties declared directly inside the block opened by `selector`. */
function declared(selector: string): Map<string, string> {
  const open = css.indexOf(`${selector} {`);
  assert.notEqual(open, -1, `tokens.css has no ${selector} block`);
  const body = css.slice(open, css.indexOf('\n}', open));
  const values = new Map<string, string>();
  for (const match of body.matchAll(/(--[a-z0-9-]+):\s*([^;]+);/g))
    values.set(match[1]!, match[2]!.replace(/\s+/g, ' ').trim());
  return values;
}

const dark = declared(':root');
const light = declared(":root[data-theme='light']");

test('every canvas role names a token defined by the default theme', () => {
  for (const token of Object.values(canvasTokens))
    assert.ok(dark.has(token), `${token} is missing from :root`);
});

test('the light theme overrides every color the default theme defines', () => {
  const colors = [...dark].filter(([, value]) => /^(#|rgba?\()/.test(value));
  assert.ok(colors.length > 30, 'expected the full color set');
  for (const [token] of colors)
    assert.ok(light.has(token), `${token} has no light value`);
});

test('tokens avoid color functions that differ across webview engines', () => {
  assert.doesNotMatch(css, /color-mix\(|\bfrom\s+(#|var\()|oklch\(/);
});

test('reading a palette resolves trimmed values for every role', () => {
  const palette = readCanvasPalette({
    getPropertyValue: (name) => ` ${dark.get(name) ?? ''} `,
  });
  assert.equal(palette.background, dark.get('--surface-timeline'));
  assert.equal(palette.playhead, dark.get('--playhead'));
  assert.match(palette.fontFamily, /Inter Variable/);
});

test('a missing token is reported by name instead of drawing black', () => {
  assert.throws(
    () =>
      readCanvasPalette({
        getPropertyValue: (name) => (name === '--cut' ? '' : '#000'),
      }),
    /--cut is not defined/,
  );
});
