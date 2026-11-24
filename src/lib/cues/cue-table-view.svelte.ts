/* eslint-disable svelte/prefer-svelte-reactivity -- Derived collections are rebuilt from immutable inputs, never mutated in place. */
import type { Cue } from '../client';
import { activeCueIds } from './cue-following';
import {
  defaultColumns,
  filterCueRows,
  metricColumns,
  rowHeight,
  visibleColumns,
  type CueColumns,
} from './cue-columns';

export class CueTableView {
  private offset = $state(0);
  private size = $state(150);
  private alive = true;
  private defaults = defaultColumns();
  query = $state('');
  issuesOnly = $state(false);
  private pinned = $state<number | null>(null);
  get resolvedCue() {
    return this.issuesOnly &&
      this.pinned === this.effects.selected?.() &&
      !this.effects.issues?.().has(String(this.pinned))
      ? this.pinned
      : null;
  }
  readonly rows = $derived.by(() =>
    filterCueRows(
      this.effects.cues(),
      this.query,
      this.issuesOnly,
      this.effects.issues?.() ?? new Map(),
      this.resolvedCue,
    ).filter((row) => this.effects.reviewIds?.()?.has(row.cue.id) ?? true),
  );
  readonly order = $derived(this.rows.map((row) => row.cue));
  private indices = $derived(
    new Map(this.rows.map((row, index) => [row.cue.id, index])),
  );
  constructor(
    private effects: {
      cues: () => Cue[];
      position: () => number | null;
      columns?: () => CueColumns;
      attached?: () => boolean;
      reviewFilter?: () => string;
      reviewIds?: () => ReadonlySet<number> | undefined;
      styled?: () => boolean;
      issues?: () => ReadonlyMap<string, unknown>;
      selected?: () => number | null;
      indexAvailable?: () => boolean;
      indexCurrent?: () => boolean;
    },
  ) {
    $effect(() => {
      const selected = this.effects.selected?.() ?? null;
      if (
        !this.issuesOnly ||
        !this.effects.indexAvailable?.() ||
        selected === null
      )
        this.pinned = null;
      else if (this.effects.issues?.().has(String(selected)))
        this.pinned = selected;
      else if (selected !== this.pinned) this.pinned = null;
    });
  }
  get filteredScopeIds() {
    if (this.issuesOnly && !this.effects.indexCurrent?.()) return [];
    return this.order
      .filter((cue) => cue.id !== this.resolvedCue)
      .map((cue) => cue.id);
  }
  get columns() {
    return this.effects.columns?.() ?? this.defaults;
  }
  get visibleColumns() {
    return visibleColumns(
      this.columns,
      this.effects.styled?.() ?? false,
      this.effects.attached?.(),
    );
  }
  get needsMetrics() {
    return this.visibleColumns.some((c) => metricColumns.has(c.id));
  }
  get rowHeight() {
    return rowHeight(this.columns.density);
  }
  get filtered() {
    return !!this.query.trim() || this.issuesOnly;
  }
  get filterKey() {
    return `${this.query}/${this.issuesOnly}/${this.effects.reviewFilter?.()}/${this.rowHeight}`;
  }
  clearFilter() {
    if (!this.alive) return;
    this.query = '';
    this.issuesOnly = false;
  }
  indexOf(id: number | null) {
    return id === null ? -1 : (this.indices.get(id) ?? -1);
  }
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
    return Math.max(0, Math.floor(this.offset / this.rowHeight) - 3);
  }
  get visibleRows() {
    return this.rows.slice(
      this.firstRow,
      this.firstRow + Math.ceil(this.size / this.rowHeight) + 6,
    );
  }
  get visibleCues() {
    return this.visibleRows.map((row) => row.cue);
  }
  get metricOffsets() {
    return [
      ...new Set(
        this.visibleRows.map((row) => Math.floor(row.index / 200) * 200),
      ),
    ].slice(0, 32);
  }
  get activeIds() {
    return activeCueIds(this.effects.cues(), this.effects.position());
  }
  dispose() {
    this.alive = false;
  }
}
