import {
  arrangementGeometry,
  refusal,
  presetArrangement,
  type Arrangement,
  type Panel,
  movePanel,
} from './arrangement';
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
  visiblePanels?: () => { review: boolean; translation: boolean };
  arranged?: (panels: Panel[]) => void;
}
/** Mounted regions stay with their owners. Only placement and user sizes change. */
export class WorkspaceLayout {
  private current = $state<Layout>(defaultLayout());
  private userLayout: Layout = defaultLayout();
  private project = $state(false);
  private transition: Promise<void> | null = null;
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
  workspaceWidth = $state(1084);
  viewportHeight = $state(720);
  editorBarHeight = $state(28);
  editorFooterHeight = $state(28);
  qualityReadoutHeight = $state(44);
  transportHeight = $state(0);
  get custom() {
    return this.current.custom ?? null;
  }
  get remembered() {
    return this.project;
  }
  get projectSnapshot(): Layout | null {
    return this.project ? structuredClone($state.snapshot(this.current)) : null;
  }
  get arrangement(): Arrangement {
    const visible = this.effects.visiblePanels?.() ?? {
      review: false,
      translation: false,
    };
    return (
      this.custom ??
      presetArrangement(
        this.preset,
        this.wavePct,
        visible.review,
        visible.translation,
      )
    );
  }
  get space() {
    return {
      width: Math.max(0, this.workspaceWidth - 16),
      height: Math.max(
        0,
        Math.min(this.workspaceHeight, this.viewportHeight - 60) - 16,
      ),
      editor: this.panes.editorMinimum,
      transport: this.panes.transportPixels,
    };
  }
  get geometry() {
    return arrangementGeometry(this.arrangement, this.space);
  }
  panelStyle(panel: Panel) {
    if (!this.custom) return '';
    const row = this.custom.rows.findIndex((row) => row.panels.includes(panel));
    if (row < 0)
      return panel === 'video'
        ? 'position:absolute;width:0;height:0;pointer-events:none;'
        : 'display:none;';
    const value = this.custom.rows[row]!;
    const column = value.panels.indexOf(panel);
    const start = column === 0 ? 1 : value.split + 1;
    const end =
      value.panels.length === 1 || column === 1 ? 101 : value.split + 1;
    return `grid-area:${row + 1}/${start}/${row + 2}/${end};${value.panels.length === 2 ? (column === 0 ? 'margin-right:3px;' : 'margin-left:3px;') : ''}`;
  }
  async arrange(value: Arrangement) {
    const reason = refusal(value, this.space);
    if (reason) throw new Error(reason);
    await this.change(() => {
      this.current.custom = structuredClone(value);
      this.effects.arranged?.(value.rows.flatMap((row) => row.panels));
    });
  }
  async rememberForProject(value: boolean) {
    if (value === this.project) return;
    await this.change(() => {
      this.project = value;
      if (!value) this.current = structuredClone(this.userLayout);
      this.publishVisibility();
    }, false);
  }
  async restoreProject(
    value: Layout | null | undefined,
    check: () => void = () => {},
  ) {
    await this.change(() => {
      check();
      this.project = !!value;
      this.current = structuredClone(
        value ? $state.snapshot(value) : this.userLayout,
      );
      this.publishVisibility();
    }, false);
  }
  async synchronizeDocks(review: boolean, translation: boolean) {
    if (!this.custom || this.active) return;
    let next = structuredClone($state.snapshot(this.custom));
    for (const [panel, visible] of [
      ['review', review],
      ['translation', translation],
    ] as const) {
      const exists = next.rows.some((row) => row.panels.includes(panel));
      if (exists === visible) continue;
      if (!visible) {
        if (next.rows.flatMap((row) => row.panels).length === 1) {
          await this.select('timing');
          return;
        }
        next = movePanel(next, { panel, kind: 'hide' });
      } else {
        const cueRow = next.rows.find(
          (row) => row.panels.length === 1 && row.panels[0] === 'cues',
        );
        if (cueRow) {
          cueRow.panels.push(panel);
          cueRow.split = 58;
        } else next.rows.push({ panels: [panel], split: 50, weight: 20 });
      }
    }
    if (JSON.stringify(next) === JSON.stringify($state.snapshot(this.custom)))
      return;
    if (refusal(next, this.space)) {
      // Opening a tool must reveal its actual owner. A standard dock is the
      // bounded fallback when a user's hidden panel cannot fit the viewport.
      await this.select(
        review ? 'review' : translation ? 'translation' : 'timing',
      );
    } else await this.arrange(next);
  }
  private publishVisibility() {
    if (this.custom)
      this.effects.arranged?.(this.custom.rows.flatMap((row) => row.panels));
    else this.effects.preset(this.preset);
  }
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
      this.userLayout = structuredClone($state.snapshot(value));
      this.publishVisibility();
    }, false).catch(() => {});
  }
  async select(preset: LayoutPreset) {
    await this.change(() => {
      this.current.custom = null;
      this.current.active = preset;
      this.effects.preset(preset);
    }).catch(() => {});
  }
  async reset() {
    await this.change(() => {
      this.current.custom = null;
      this.effects.preset(this.preset);
      this.current[this.preset] = {
        ...structuredClone(this.description.defaults),
        columns: this.current[this.preset].columns,
      };
    }).catch(() => {});
  }
  resizeWaveform(value: number) {
    void this.change(() => {
      this.current[this.preset].wavePct = clamp(value, 20, 80);
    }).catch(() => {});
  }
  resizePreview(value: number) {
    void this.change(() => {
      this.current[this.preset].topPct = clamp(value, 10, 65);
    }).catch(() => {});
  }
  resizeEditor(value: number) {
    void this.change(() => {
      this.current[this.preset].editorPct = clamp(100 - value, 10, 45);
    }).catch(() => {});
  }
  private change(apply: () => void, persist = true): Promise<void> {
    if (!this.alive) return Promise.resolve();
    this.saveRequested ||= persist;
    this.next = apply;
    if (this.transition) return this.transition;
    this.changing = true;
    this.transition = this.drain().finally(() => {
      this.changing = false;
      this.transition = null;
    });
    return this.transition;
  }
  private async drain() {
    try {
      // Coalesce geometry while hiding, but wait through restore too. Project
      // restore callers await this same lifetime before publishing readiness.
      do {
        await this.video.open(() => {});
        while (this.alive && this.next) {
          const next = this.next;
          this.next = null;
          next();
          await this.effects.afterRender();
        }
        let failure: unknown;
        await this.video.close({
          hide: () => {},
          failed: (error) => {
            failure = error;
          },
        });
        if (failure !== undefined) throw failure;
      } while (this.alive && this.next);
      if (this.alive && this.saveRequested) {
        this.saveRequested = false;
        this.scheduleSave();
      }
    } catch (error) {
      this.next = null;
      this.saveRequested = false;
      if (this.alive) {
        this.effects.error(error);
        await this.video.close({ hide: () => {}, failed: this.effects.error });
      }
      throw error;
    }
  }
  private scheduleSave() {
    if (this.project) return;
    this.userLayout = structuredClone($state.snapshot(this.current));
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      this.timer = null;
      this.pendingSave = structuredClone(this.userLayout);
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
