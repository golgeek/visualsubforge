import { TimingReviewSelection } from '../timing/timing-review-selection.svelte';
export type TimingKind = 'gap' | 'scene' | 'duration' | 'all' | 'batch';
/** Only workspace panel visibility and its existing open/close rules. */
export class WorkspaceView {
  translationOpen = $state(false);
  private reviewBeforeTranslation = false;
  problemsOpen = $state(false);
  diagnostics = $state(false);
  reviewBusy = $state(false);
  reviewTab = $state<'issues' | 'timing' | 'statistics' | 'changes'>('issues');
  timingMode = $state<'fix' | 'shift'>('fix');
  families = $state({ gaps: true, speed: true, shots: true });
  timingSelection = $state.raw(new TimingReviewSelection());
  policyOpen = $state(false);
  openReview(
    tab: 'issues' | 'timing' | 'statistics' | 'changes',
    preset?: TimingKind,
  ) {
    if (this.reviewBusy) return;
    this.reviewTab = tab;
    if (preset) {
      this.timingMode = preset === 'batch' ? 'shift' : 'fix';
      if (preset !== 'batch')
        this.families = {
          gaps: preset === 'all' || preset === 'gap',
          speed: preset === 'all' || preset === 'duration',
          shots: preset === 'all' || preset === 'scene',
        };
    }
    this.translationOpen = false;
    this.problemsOpen = true;
  }
  toggleProblems() {
    if (this.problemsOpen && this.reviewTab !== 'issues') {
      this.reviewTab = 'issues';
      return;
    }
    this.reviewTab = 'issues';
    this.problemsOpen = !this.problemsOpen;
    if (this.problemsOpen) this.translationOpen = false;
  }
  closeProblems() {
    if (this.reviewBusy) return;
    this.problemsOpen = false;
  }
  toggleTranslation() {
    if (this.reviewBusy) return;
    this.translationOpen = !this.translationOpen;
    if (this.translationOpen) {
      this.reviewBeforeTranslation = this.problemsOpen;
      this.problemsOpen = false;
    } else {
      this.problemsOpen = this.reviewBeforeTranslation;
      this.reviewBeforeTranslation = false;
    }
  }
  revealProblems() {
    this.reviewTab = 'issues';
    // Statistics navigation preserves the existing translation-panel visibility.
    this.problemsOpen = true;
  }
}
