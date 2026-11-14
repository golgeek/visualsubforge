export type TimingKind = 'gap' | 'scene' | 'duration' | 'batch';
/** Only workspace panel visibility and its existing open/close rules. */
export class WorkspaceView {
  translationOpen = $state(false);
  problemsOpen = $state(false);
  diagnostics = $state(false);
  reviewBusy = $state(false);
  reviewTab = $state<'issues' | 'timing' | 'shift' | 'statistics'>('issues');
  timingKind = $state<'gap' | 'duration' | 'scene'>('gap');
  openReview(
    tab: 'issues' | 'timing' | 'shift' | 'statistics',
    kind?: 'gap' | 'duration' | 'scene',
  ) {
    if (this.reviewBusy) return;
    this.reviewTab = tab;
    if (kind) this.timingKind = kind;
    this.translationOpen = false;
    this.problemsOpen = true;
  }
  toggleProblems(cancelNavigation: () => void) {
    if (this.problemsOpen && this.reviewTab !== 'issues') {
      this.reviewTab = 'issues';
      return;
    }
    this.reviewTab = 'issues';
    this.problemsOpen = !this.problemsOpen;
    if (this.problemsOpen) this.translationOpen = false;
    if (!this.problemsOpen) cancelNavigation();
  }
  closeProblems(cancelNavigation: () => void) {
    if (this.reviewBusy) return;
    this.problemsOpen = false;
    cancelNavigation();
  }
  toggleTranslation() {
    this.translationOpen = !this.translationOpen;
    // Hiding Problems from this toggle does not resolve its pending choice.
    if (this.translationOpen) this.problemsOpen = false;
  }
  revealProblems() {
    this.reviewTab = 'issues';
    // Statistics navigation preserves the existing translation-panel visibility.
    this.problemsOpen = true;
  }
}
