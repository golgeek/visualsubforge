// Only review choices and a viewport bookmark survive a tab switch.
// Session captures, proposal pages and worker identities stay in the mounted panel.
export class TimingReviewSelection {
  selectionKey = '';
  toggled = $state<number[]>([]);
  openRows = $state<number[]>([]);
  manualOpen = $state(true);
  manualOffset = $state(0);
  changesOffset = 0;
  anchor: { key: string; index: number; offset: number } | null = null;
  constructor(key = '') {
    this.selectionKey = key;
  }
}
