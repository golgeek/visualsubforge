import type { Preferences } from '../client';

type Settings = Preferences['appearance'];
interface SystemTheme {
  readonly matches: boolean;
  addEventListener: (event: 'change', listener: () => void) => void;
  removeEventListener: (event: 'change', listener: () => void) => void;
}
/** One system-theme listener per workspace. Only confirmed settings reach here. */
export class Appearance {
  private alive = true;
  private value: Settings | null = null;
  private initialTheme: string | null;
  private initialDensity: string | null;
  constructor(
    private root = document.documentElement,
    private system: SystemTheme = window.matchMedia(
      '(prefers-color-scheme: dark)',
    ),
  ) {
    this.initialTheme = root.getAttribute('data-theme');
    this.initialDensity = root.getAttribute('data-density');
    system.addEventListener('change', this.systemChanged);
  }
  apply(value: Settings) {
    if (!this.alive) return;
    this.value = { ...value };
    this.render();
  }
  private systemChanged = () => {
    if (this.alive && this.value?.theme === 'system') this.render();
  };
  private render() {
    if (!this.value) return;
    const theme =
      this.value.theme === 'system'
        ? this.system.matches
          ? 'dark'
          : 'light'
        : this.value.theme;
    if (this.root.dataset.theme !== theme) this.root.dataset.theme = theme;
    if (this.root.dataset.density !== this.value.density)
      this.root.dataset.density = this.value.density;
  }
  dispose() {
    if (!this.alive) return;
    this.alive = false;
    this.system.removeEventListener('change', this.systemChanged);
    for (const [name, value] of [
      ['data-theme', this.initialTheme],
      ['data-density', this.initialDensity],
    ] as const) {
      if (value === null) this.root.removeAttribute(name);
      else this.root.setAttribute(name, value);
    }
  }
}
