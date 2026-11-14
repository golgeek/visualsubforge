/**
 * Canvas drawing cannot use CSS custom properties directly, so canvases read
 * resolved token values into a plain palette and redraw when it changes.
 * tokens.css stays the single source of color; this module only names which
 * token each canvas role uses.
 */

export interface CanvasPalette {
  warning: string;
  error: string;
  accent: string;
  fontMono: string;
  background: string;
  rulerLine: string;
  rulerText: string;
  wave: string;
  cueFill: string;
  cueFillSelected: string;
  cueLine: string;
  cueLineSelected: string;
  cueText: string;
  cueBuffer: string;
  cut: string;
  cutBuffer: string;
  rangeFill: string;
  rangeLine: string;
  playhead: string;
  fontFamily: string;
}

/** The custom property behind each palette role. */
export const canvasTokens: Readonly<Record<keyof CanvasPalette, string>> = {
  warning: '--warn',
  error: '--err',
  accent: '--accent',
  fontMono: '--font-mono',
  background: '--surface-timeline',
  rulerLine: '--line',
  rulerText: '--text-muted',
  wave: '--wave',
  cueFill: '--cue-fill',
  cueFillSelected: '--cue-fill-selected',
  cueLine: '--cue-line',
  cueLineSelected: '--cue-line-selected',
  cueText: '--cue-text',
  cueBuffer: '--buffer-cue',
  cut: '--cut',
  cutBuffer: '--cut-soft',
  rangeFill: '--range-fill',
  rangeLine: '--range-line',
  playhead: '--playhead',
  fontFamily: '--font-ui',
};

/** Structural subset of CSSStyleDeclaration, so tests need no DOM. */
export interface TokenSource {
  getPropertyValue(name: string): string;
}

/**
 * Resolves every role. A missing token is a programming error (a renamed or
 * unloaded tokens.css), reported with the property name rather than drawn as
 * the canvas default black.
 */
export function readCanvasPalette(source: TokenSource): CanvasPalette {
  const palette = {} as CanvasPalette;
  for (const [role, token] of Object.entries(canvasTokens) as [
    keyof CanvasPalette,
    string,
  ][]) {
    const value = source.getPropertyValue(token).trim();
    if (!value) throw new Error(`Theme token ${token} is not defined`);
    palette[role] = value;
  }
  return palette;
}

/**
 * Calls `onchange` when resolved token values may have changed: the root
 * theme attribute switched, or a web font finished loading (canvas text does
 * not re-render by itself). Returns the cleanup for the owning component.
 */
export function watchCanvasPalette(
  root: HTMLElement,
  onchange: () => void,
): () => void {
  const observer = new MutationObserver(onchange);
  observer.observe(root, { attributes: true, attributeFilter: ['data-theme'] });
  const fonts = root.ownerDocument.fonts;
  fonts.addEventListener('loadingdone', onchange);
  return () => {
    observer.disconnect();
    fonts.removeEventListener('loadingdone', onchange);
  };
}
