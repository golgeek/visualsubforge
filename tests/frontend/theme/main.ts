import { mount, tick } from 'svelte';
import tokensSource from '../../../src/lib/ui/tokens.css?raw';
import { readCanvasPalette } from '../../../src/lib/ui/canvas-palette';
import Reference from './Reference.svelte';

// Style reference for the workspace theme. Open it by URL to review both
// themes; the checks below keep the palette legible when tokens change.
// `?theme=light` opens the light palette for manual review.

const heading = document.querySelector('#result')!;
const output = document.querySelector('#checks')!;
const root = document.documentElement;

function assert(value: unknown, message: string): asserts value {
  if (!value) throw new Error(message);
}
function passed(message: string) {
  const row = document.createElement('li');
  row.textContent = `PASS: ${message}`;
  output.append(row);
}

const tokens = [
  ...new Set(
    [
      ...tokensSource
        .replace(/\/\*[\s\S]*?\*\//g, '')
        .matchAll(/(--[a-z0-9-]+):/g),
    ].map((m) => m[1]!),
  ),
];

type Rgba = [number, number, number, number];

/** Resolves any CSS color through the engine, so parsing matches rendering. */
function rgba(color: string): Rgba {
  const probe = document.createElement('canvas').getContext('2d')!;
  probe.fillStyle = '#000';
  probe.fillStyle = color;
  const value = String(probe.fillStyle);
  if (value.startsWith('#')) {
    const n = Number.parseInt(value.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
  }
  const parts = /rgba?\(([^)]+)\)/.exec(value)![1]!.split(',').map(Number);
  return [parts[0]!, parts[1]!, parts[2]!, parts[3] ?? 1];
}
function token(name: string): string {
  const value = getComputedStyle(root).getPropertyValue(name).trim();
  assert(value, `${name} resolves`);
  return value;
}
function over(top: Rgba, under: Rgba): Rgba {
  const a = top[3];
  return [0, 1, 2]
    .map((i) => top[i]! * a + under[i]! * (1 - a))
    .concat(1) as Rgba;
}
function luminance([r, g, b]: Rgba): number {
  const channel = (v: number) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}
/** WCAG contrast of `fg` drawn on `bg`, itself drawn on an opaque `under`. */
function contrast(fg: string, bg: string, under?: string): number {
  let back = rgba(token(bg));
  if (under) back = over(back, rgba(token(under)));
  const front = over(rgba(token(fg)), back);
  const [hi, lo] = [luminance(front), luminance(back)].sort((a, b) => b - a);
  return (hi! + 0.05) / (lo! + 0.05);
}

// [foreground, background, minimum ratio, optional opaque surface below].
// 4.5 is WCAG AA for text; 3 for non-text indicators and the faint tier,
// which only carries disabled or redundant information.
const pairs: [string, string, number, string?][] = [
  ['--text', '--surface-base', 4.5],
  ['--text', '--surface-panel', 4.5],
  ['--text', '--surface-hover', 4.5],
  ['--text-muted', '--surface-base', 4.5],
  ['--text-muted', '--surface-panel', 4.5],
  ['--text-muted', '--surface-raised', 4.5],
  ['--text-muted', '--surface-hover', 4.5],
  ['--text-muted', '--surface-timeline', 4.5],
  ['--text-faint', '--surface-panel', 3],
  ['--accent-ink', '--accent', 4.5],
  ['--accent-text', '--surface-panel', 4.5],
  ['--accent-text', '--surface-raised', 4.5],
  ['--ok', '--surface-panel', 4.5],
  ['--warn', '--surface-panel', 4.5],
  ['--err', '--surface-panel', 4.5],
  ['--err', '--surface-raised', 4.5],
  ['--err', '--err-soft', 4.5, '--surface-panel'],
  ['--warn', '--warn-soft', 4.5, '--surface-panel'],
  ['--text', '--selection', 4.5, '--surface-panel'],
  ['--text', '--warn-soft', 4.5, '--surface-base'],
  ['--cue-text', '--cue-fill-selected', 4.5, '--surface-timeline'],
  ['--focus', '--surface-base', 3],
  ['--focus', '--surface-panel', 3],
  ['--playhead', '--surface-timeline', 3],
  ['--cut', '--surface-timeline', 3],
  ['--wave', '--surface-timeline', 3],
  ['--text-on-video', '--surface-video', 4.5],
  ['--cue-line-selected', '--surface-timeline', 3],
];

async function paint() {
  await tick();
  await new Promise<void>((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
  );
}

/** Background pixel of the waveform canvas, away from cues, ticks and labels. */
function canvasBackground(): Rgba {
  const canvas = document.querySelector<HTMLCanvasElement>('.wave canvas');
  assert(canvas, 'Waveform canvas is mounted');
  const scale = canvas.width / canvas.getBoundingClientRect().width;
  const x = Math.round(60 * scale);
  const y = Math.round(5 * scale);
  const data = canvas.getContext('2d')!.getImageData(x, y, 1, 1).data;
  return [data[0]!, data[1]!, data[2]!, 1];
}

async function checkTheme(theme: 'dark' | 'light') {
  if (theme === 'light') root.dataset.theme = 'light';
  else delete root.dataset.theme;
  await paint();

  const missing = tokens.filter(
    (name) => !getComputedStyle(root).getPropertyValue(name).trim(),
  );
  assert(!missing.length, `${theme}: unresolved ${missing.join(', ')}`);
  passed(`${theme}: all ${tokens.length} tokens resolve`);

  for (const [fg, bg, minimum, under] of pairs) {
    const ratio = contrast(fg, bg, under);
    assert(
      ratio >= minimum,
      `${theme}: ${fg} on ${bg}${under ? ` over ${under}` : ''} is ${ratio.toFixed(2)}:1, needs ${minimum}:1`,
    );
  }
  passed(`${theme}: ${pairs.length} color pairs meet their contrast minimum`);

  const palette = readCanvasPalette(getComputedStyle(root));
  const expected = rgba(palette.background);
  const actual = canvasBackground();
  assert(
    expected.slice(0, 3).every((v, i) => Math.abs(v - actual[i]!) <= 1),
    `${theme}: waveform background is rgb(${actual.slice(0, 3).join(',')}), expected rgb(${expected.slice(0, 3).join(',')})`,
  );
  passed(`${theme}: waveform canvas redraws with the theme palette`);
}

async function run() {
  const host = document.querySelector<HTMLElement>('#test')!;
  mount(Reference, { target: host, props: { tokens } });
  try {
    await document.fonts.ready;
    await document.fonts.load('13px "Inter Variable"');
    await document.fonts.load('13px "JetBrains Mono Variable"');
    assert(
      document.fonts.check('13px "Inter Variable"') &&
        document.fonts.check('13px "JetBrains Mono Variable"'),
      'Bundled interface and timecode fonts load',
    );
    passed('Bundled Inter and JetBrains Mono load');
    await checkTheme('dark');
    await checkTheme('light');
    // Leave the page in the theme a reviewer asked for.
    if (new URLSearchParams(location.search).get('theme') !== 'light')
      delete root.dataset.theme;
    heading.textContent = `PASS: all ${output.childElementCount} theme checks`;
  } catch (e) {
    heading.textContent = 'FAIL: ' + String(e);
  }
}
void run();
