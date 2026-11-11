import type { Cue } from './client';
import { activeCueIds } from './cue-following';

export class CueTableView {
  private offset = $state(0);
  private size = $state(150);
  private alive = true;
  constructor(
    private effects: { cues: () => Cue[]; position: () => number | null },
  ) {}
  get scrollTop() {
    return this.offset;
  }
  set scrollTop(value: number) {
    if (this.alive) this.offset = value;
  }
  get height() {
    return this.size;
  }
  set height(value: number) {
    if (this.alive) this.size = value;
  }
  get firstRow() {
    return Math.max(0, Math.floor(this.offset / 32) - 3);
  }
  get visibleCues() {
    return this.effects
      .cues()
      .slice(this.firstRow, this.firstRow + Math.ceil(this.size / 32) + 6);
  }
  get activeIds() {
    return activeCueIds(this.effects.cues(), this.effects.position());
  }
  dispose() {
    this.alive = false;
  }
}
