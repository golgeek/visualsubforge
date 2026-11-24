import { afterSettle } from '../ui/settle-delay';

export class WorkspaceActivity {
  visible = $state(false);
  private count = 0;
  private disposed = false;
  private cancelShow?: () => void;
  private hide?: ReturnType<typeof setTimeout>;
  private shownAt = 0;

  begin() {
    if (this.disposed) return () => {};
    if (++this.count === 1) {
      clearTimeout(this.hide);
      if (!this.visible)
        this.cancelShow = afterSettle(() => {
          this.shownAt = performance.now();
          this.visible = true;
        });
    }
    let ended = false;
    return () => {
      if (ended || this.disposed) return;
      ended = true;
      if (--this.count) return;
      this.cancelShow?.();
      if (!this.visible) return;
      const remaining = 300 - (performance.now() - this.shownAt);
      if (remaining <= 0) this.visible = false;
      else this.hide = setTimeout(() => (this.visible = false), remaining);
    };
  }
  track<T>(work: Promise<T>) {
    return work.finally(this.begin());
  }
  dispose() {
    this.disposed = true;
    this.cancelShow?.();
    clearTimeout(this.hide);
    this.visible = false;
  }
}
