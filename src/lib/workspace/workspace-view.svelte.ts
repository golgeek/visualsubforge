/** Only workspace panel visibility and its existing open/close rules. */
export class WorkspaceView {
  translationOpen = $state(false);
  problemsOpen = $state(false);
  diagnostics = $state(false);
  toggleProblems(cancelNavigation: () => void) {
    this.problemsOpen = !this.problemsOpen;
    if (this.problemsOpen) this.translationOpen = false;
    if (!this.problemsOpen) cancelNavigation();
  }
  closeProblems(cancelNavigation: () => void) {
    this.problemsOpen = false;
    cancelNavigation();
  }
  toggleTranslation() {
    this.translationOpen = !this.translationOpen;
    // Hiding Problems from this toggle does not resolve its pending choice.
    if (this.translationOpen) this.problemsOpen = false;
  }
  revealProblems() {
    // Statistics navigation preserves the existing translation-panel visibility.
    this.problemsOpen = true;
  }
}
