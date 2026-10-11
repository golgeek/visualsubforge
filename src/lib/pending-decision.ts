// Shared by the two workspace save dialogs. Presentation and native occlusion
// remain in App; this owner only settles the outstanding decision exactly once.
export class PendingDecision<Choice> {
  private resolve: ((choice: Choice) => void) | null = null;
  private disposed = false;
  private cancelled: Choice;

  constructor(cancelled: Choice) {
    this.cancelled = cancelled;
  }
  get pending() {
    return this.resolve !== null;
  }
  wait(): Promise<Choice> {
    if (this.disposed) return Promise.resolve(this.cancelled);
    if (this.resolve) throw new Error('A dialog decision is already pending.');
    return new Promise((resolve) => {
      this.resolve = resolve;
    });
  }
  answer(choice: Choice) {
    const resolve = this.resolve;
    this.resolve = null;
    resolve?.(choice);
  }
  dispose() {
    this.disposed = true;
    this.answer(this.cancelled);
  }
}
