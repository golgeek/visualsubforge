import { scenesView } from '../client';

interface Input {
  session: number;
  generation: number;
  duration: number;
  total: number;
  partial: boolean;
}
/** Read-only full-file projection. One request at a time, bounded subdivisions, no scan ownership. */
export class SceneOverview {
  cuts = $state.raw<number[]>([]);
  incomplete = $state(true);
  error = $state('');
  private epoch = 0;
  private key = '';
  private desired: Input | null = null;
  private pending = false;
  private alive = true;
  constructor(private read = scenesView) {}
  configure(input: Input | null) {
    const key = JSON.stringify(input);
    if (!this.alive || key === this.key) return;
    this.key = key;
    this.desired = input;
    ++this.epoch;
    this.cuts = [];
    this.incomplete = true;
    this.error = '';
    void this.load();
  }
  private async load() {
    if (this.pending || !this.alive || !this.desired) return;
    this.pending = true;
    const epoch = this.epoch,
      input = this.desired;
    const windows: [number, number][] = [[0, input.duration]];
    // Request-local accumulator; only its immutable array is published.
    // eslint-disable-next-line svelte/prefer-svelte-reactivity
    const cuts = new Set<number>();
    let requests = 0,
      incomplete = input.partial;
    try {
      while (windows.length && requests < 128 && cuts.size < 100000) {
        const [start, end] = windows.shift()!;
        const view = await this.read(input.generation, start, end, 0);
        ++requests;
        if (!this.alive || epoch !== this.epoch) return;
        if (
          view.generation !== input.generation ||
          view.running ||
          view.mode !== 'visual' ||
          view.error
        )
          throw new Error(
            view.error ||
              'Shot-change overview changed; waiting for current scan',
          );
        if (view.truncated && end - start > 0.001) {
          const middle = (start + end) / 2;
          windows.push([start, middle], [middle, end]);
        } else {
          incomplete ||= view.truncated || view.partial;
          for (const cut of view.cuts)
            if (Number.isFinite(cut) && cut >= 0 && cut <= input.duration) {
              if (cuts.size >= 100000) {
                incomplete = true;
                break;
              }
              cuts.add(cut);
            }
        }
      }
      if (!this.alive || epoch !== this.epoch) return;
      this.cuts = [...cuts].sort((a, b) => a - b);
      this.incomplete =
        incomplete || windows.length > 0 || cuts.size !== input.total;
    } catch (error) {
      if (this.alive && epoch === this.epoch) {
        this.error = String(error);
        this.incomplete = true;
      }
    } finally {
      this.pending = false;
      if (this.alive && epoch !== this.epoch) void this.load();
    }
  }
  dispose() {
    this.alive = false;
    ++this.epoch;
    this.desired = null;
  }
}
