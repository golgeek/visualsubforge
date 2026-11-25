import { tick } from 'svelte';
import { afterSettle } from '../ui/settle-delay';

export class WorkspaceActivity {
  visible = $state(false);
  private count = 0;
  private disposed = false;
  private cancelShow?: () => void;
  private holdTimer?: ReturnType<typeof setTimeout>;
  private minimumElapsed = false;

  private async show() {
    this.minimumElapsed = false;
    this.visible = true;
    // Start the minimum display time after Svelte commits the footer class.
    await tick();
    if (this.disposed) return;
    this.holdTimer = setTimeout(() => {
      this.minimumElapsed = true;
      if (!this.count) this.visible = false;
    }, 300);
  }

  begin() {
    if (this.disposed) return () => {};
    if (++this.count === 1 && !this.visible)
      this.cancelShow = afterSettle(() => void this.show());
    let ended = false;
    return () => {
      if (ended || this.disposed) return;
      ended = true;
      if (--this.count) return;
      this.cancelShow?.();
      if (this.minimumElapsed) this.visible = false;
    };
  }
  track<T>(work: Promise<T>) {
    return work.finally(this.begin());
  }
  dispose() {
    this.disposed = true;
    this.cancelShow?.();
    clearTimeout(this.holdTimer);
    this.visible = false;
  }
}
