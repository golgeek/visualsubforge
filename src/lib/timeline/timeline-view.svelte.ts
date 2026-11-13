import {
  waveformStart,
  waveformView,
  type WaveformView,
  type MediaStatus,
  type Cue,
} from '../client';
import { clamp, viewport } from './timeline';
import {
  playbackViewportStart,
  waveformRequestWindow,
} from './waveform-navigation';

interface TimelineEffects {
  status: () => MediaStatus;
  mediaPath: () => string;
  preview?: () => Cue | null;
  pending: () => boolean;
  cues: () => Cue[];
  gap: () => number;
  selectRange: (session: number, range: [number, number]) => void;
}

// Construct during component initialization: the effects share its lifetime.
export class TimelineView {
  follow = $state(true);
  interacting = $state(false);
  start = $state(0);
  span = $state(20);
  range = $state<[number, number] | null>(null);
  wave = $state<WaveformView | null>(null);
  generation = $state<number | null>(null);
  error = $state('');
  width = $state(600);
  private alive = true;
  private waveKey = '';
  private waveRequest = 0;
  private polling = false;
  private lastWave = '';
  private timer: ReturnType<typeof setInterval> | null = null;
  constructor(
    private effects: TimelineEffects,
    private startWave = waveformStart,
    private readWave = waveformView,
  ) {
    $effect(() => {
      if (!this.alive) return;
      const next = viewport(this.start, this.span, this.duration || 20);
      if (next.start !== this.start) this.start = next.start;
      if (next.span !== this.span) this.span = next.span;
    });
    $effect(() => {
      if (!this.alive) return;
      const track = this.effects
        .status()
        .audioTracks.find((t) => t.id === this.effects.status().audioTrack);
      const key =
        this.effects.status().loaded && this.effects.mediaPath() && track
          ? `${this.effects.status().session}:${track.id}:${this.effects.mediaPath()}`
          : '';
      if (key === this.waveKey) return;
      this.waveKey = key;
      const request = ++this.waveRequest;
      this.wave = null;
      this.error = '';
      this.generation = null;
      if (!key || !track) return;
      void this.startWave(
        this.effects.mediaPath(),
        track.audioOrdinal,
        this.effects.status().session,
      )
        .then((generation) => {
          if (this.alive && request === this.waveRequest)
            this.generation = generation;
        })
        .catch((e) => {
          if (this.alive && request === this.waveRequest)
            this.error = String(e);
        });
    });
    $effect(() => {
      if (!this.alive) return;
      const position = this.effects.status().positionSeconds;
      if (
        this.follow &&
        !this.effects.preview?.() &&
        !this.interacting &&
        !this.effects.pending() &&
        !this.effects.status().seeking &&
        this.effects.status().loaded &&
        position !== null
      ) {
        this.start = playbackViewportStart(position, this.span, this.duration);
      }
    });
  }
  connectPreview(read: () => Cue | null) {
    this.effects.preview = read;
  }
  get duration() {
    return Math.max(
      this.effects.status().durationSeconds ?? 0,
      this.wave?.durationSeconds ?? 0,
    );
  }
  get waveCues() {
    const preview = this.effects.preview?.();
    return this.effects
      .cues()
      .map((c) => (c.id === preview?.id ? preview : c))
      .filter(
        (c) =>
          (c.endMs + this.effects.gap()) / 1000 >= this.start &&
          (c.startMs - this.effects.gap()) / 1000 <= this.start + this.span,
      );
  }
  selectRange(next: [number, number]) {
    if (!this.alive) return;
    this.range = next;
    this.effects.selectRange(this.effects.status().session, next);
  }
  invalidateTrack() {
    this.waveKey = '';
  }
  reset(rewind = false) {
    ++this.waveRequest;
    this.wave = null;
    this.generation = null;
    this.range = null;
    if (rewind) this.start = 0;
  }
  restore(start: number, span: number) {
    if (!this.alive) return;
    this.span = Math.max(
      0.01,
      Math.min(span, this.effects.status().durationSeconds ?? span),
    );
    this.start = Math.max(
      0,
      Math.min(
        start,
        Math.max(0, (this.effects.status().durationSeconds ?? 0) - this.span),
      ),
    );
  }
  zoom(factor: number) {
    if (!this.alive) return;
    const center = clamp(
      this.effects.status().positionSeconds ?? this.start + this.span / 2,
      this.start,
      this.start + this.span,
    );
    const next = viewport(
      center - (this.span * factor) / 2,
      this.span * factor,
      this.duration || 20,
    );
    this.start = next.start;
    this.span = next.span;
  }
  async poll() {
    if (!this.alive || this.polling || this.generation === null) return;
    const generation = this.generation;
    const bounds = waveformRequestWindow(
      this.start,
      this.span,
      this.duration,
      this.width,
    );
    const key = JSON.stringify([generation, bounds]);
    if (key === this.lastWave && !this.wave?.running) return;
    this.polling = true;
    try {
      const next = await this.readWave(
        generation,
        bounds.start,
        bounds.end,
        bounds.pixels,
      );
      if (
        this.alive &&
        generation === this.generation &&
        key ===
          JSON.stringify([
            this.generation,
            waveformRequestWindow(
              this.start,
              this.span,
              this.duration,
              this.width,
            ),
          ])
      ) {
        this.wave = next;
        this.lastWave = key;
      }
    } catch (e) {
      if (this.alive && generation === this.generation) this.error = String(e);
    } finally {
      this.polling = false;
    }
  }

  startPolling() {
    if (!this.alive || this.timer !== null) return;
    this.timer = setInterval(() => void this.poll(), 100);
  }
  dispose() {
    this.alive = false;
    ++this.waveRequest;
    if (this.timer !== null) clearInterval(this.timer);
    this.timer = null;
  }
}
