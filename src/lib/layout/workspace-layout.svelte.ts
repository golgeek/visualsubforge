import type { CueColumns } from '../cues/cue-columns';
import { clamp } from '../timeline/timeline';
import { paneLayout } from './pane-layout';
import {
  defaultLayout,
  layoutPresets,
  type LayoutPreset,
} from './layout-presets';
import { HideVideo, type HideVideoEffects } from '../dialogs/hide-video.svelte';
import { saveLayout, type Preferences } from '../client';
import type { WorkspacePreferences } from '../settings/workspace-preferences.svelte';
import type { DocumentSession } from '../document/document-session.svelte';

type Layout = Preferences['layout'];
interface Effects extends HideVideoEffects {
  preset: (preset: LayoutPreset) => void;
  error: (error: unknown) => void;
}
/** Mounted regions stay with their owners. Only placement and user sizes change. */
export class WorkspaceLayout {
  private current = $state<Layout>(defaultLayout());
  private video: HideVideo;
  private alive = true;
  private initialized = false;
  private changing = $state(false);
  private next: (() => void) | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private saving = false;
  private saveRequested = false;
  private pendingSave: Layout | null = null;
  constructor(
    private preferences: WorkspacePreferences,
    private subtitles: Pick<DocumentSession, 'cue'>,
    private effects: Effects,
    private write: typeof saveLayout = saveLayout,
  ) {
    this.video = new HideVideo(effects);
  }
  workspaceHeight = $state(600);
  editorBarHeight = $state(0);
  editorFooterHeight = $state(0);
  qualityReadoutHeight = $state(0);
  transportHeight = $state(0);
  get active() {
    return this.changing;
  }
  get preset() {
    return this.current.active;
  }
  get description() {
    return layoutPresets[this.preset];
  }
  get columns() {
    return this.current[this.preset].columns;
  }
  setColumns(value: CueColumns) {
    if (!this.alive || this.changing) return;
    // Column geometry stays inside the cue list. The chooser owns its own
    // HideVideo lifetime; reuse the existing save queue without moving panes.
    this.current[this.preset].columns = structuredClone($state.snapshot(value));
    this.scheduleSave();
  }
  get topPct() {
    return this.current[this.preset].topPct;
  }
  get editorPct() {
    return this.current[this.preset].editorPct;
  }
  get wavePct() {
    return this.current[this.preset].wavePct;
  }
  initialize(value: Layout) {
    if (this.initialized) return;
    this.initialized = true;
    if (JSON.stringify(value) === JSON.stringify($state.snapshot(this.current)))
      return;
    void this.change(() => {
      this.current = structuredClone($state.snapshot(value));
      this.effects.preset(this.preset);
    }, false);
  }
  async select(preset: LayoutPreset) {
    await this.change(() => {
      this.current.active = preset;
      this.effects.preset(preset);
    });
  }
  async reset() {
    await this.change(() => {
      this.current[this.preset] = {
        ...structuredClone(this.description.defaults),
        columns: this.current[this.preset].columns,
      };
    });
  }
  resizeWaveform(value: number) {
    void this.change(() => {
      this.current[this.preset].wavePct = clamp(value, 20, 80);
    });
  }
  resizePreview(value: number) {
    void this.change(() => {
      this.current[this.preset].topPct = clamp(value, 10, 65);
    });
  }
  resizeEditor(value: number) {
    void this.change(() => {
      this.current[this.preset].editorPct = clamp(100 - value, 10, 45);
    });
  }
  private async change(apply: () => void, persist = true) {
    if (!this.alive) return;
    // Rapid pointer movement coalesces to the latest size while native hiding
    // is pending. No DOM position changes before that acknowledgment.
    this.saveRequested ||= persist;
    this.next = apply;
    if (this.changing) return;
    this.changing = true;
    try {
      await this.video.open(() => {});
      while (this.alive && this.next) {
        const next = this.next;
        this.next = null;
        next();
        await this.effects.afterRender();
      }
      await this.video.close({ hide: () => {}, failed: this.effects.error });
      if (this.alive && this.saveRequested) {
        this.saveRequested = false;
        this.scheduleSave();
      }
    } catch (error) {
      this.next = null;
      if (this.alive) {
        this.effects.error(error);
        await this.video.close({ hide: () => {}, failed: this.effects.error });
      }
    } finally {
      this.changing = false;
      // Input arriving during restore needs a fresh hide lifetime.
      const next = this.next;
      this.next = null;
      if (this.alive && next) void this.change(next);
    }
  }
  private scheduleSave() {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      this.pendingSave = structuredClone($state.snapshot(this.current));
      void this.flushSave();
    }, 250);
  }
  private async flushSave() {
    if (this.saving) return;
    this.saving = true;
    try {
      while (this.alive && this.pendingSave) {
        const value = this.pendingSave;
        this.pendingSave = null;
        await this.write(value);
        // Acknowledgments never replace newer transient sizes.
        if (this.alive) this.preferences.acceptLayout(value);
      }
    } catch (error) {
      if (this.alive) this.effects.error(error);
    } finally {
      this.saving = false;
      if (this.alive && this.pendingSave) void this.flushSave();
    }
  }
  measureEditor(part: 'bar' | 'footer' | 'metrics', height: number) {
    if (part === 'bar') this.editorBarHeight = height;
    else if (part === 'footer') this.editorFooterHeight = height;
    else this.qualityReadoutHeight = height;
  }
  get panes() {
    return paneLayout({
      workspaceHeight: this.workspaceHeight,
      preset: this.preset,
      topPct: this.topPct,
      editorPct: this.editorPct,
      showQualityMetrics: this.preferences.showQualityMetrics,
      hasCue: !!this.subtitles.cue,
      editorBarHeight: this.editorBarHeight,
      editorFooterHeight: this.editorFooterHeight,
      qualityReadoutHeight: this.qualityReadoutHeight,
      transportHeight: this.transportHeight,
    });
  }
  dispose() {
    this.alive = false;
    this.next = null;
    this.pendingSave = null;
    if (this.timer !== null) clearTimeout(this.timer);
    this.video.dispose();
  }
}
