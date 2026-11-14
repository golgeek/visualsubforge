import {
  recoveryStatus,
  discardRecovery,
  type RecoveryStatus,
} from '../client';
import type { Workspace } from '../workspace/workspace.svelte';
import type { DocumentSession } from './document-session.svelte';

export class Recovery {
  status = $state<RecoveryStatus>({ entries: [], error: null });
  discardId = $state<string | null>(null);
  active = $state(false);
  private request = 0;
  private polling = false;
  private alive = true;
  private timer: ReturnType<typeof setInterval> | null = null;
  constructor(
    private workspace: Workspace,
    private subtitles: DocumentSession,
    private effects: {
      protect: () => Promise<boolean>;
      suspendFollowing: () => void;
    },
  ) {}

  startPolling() {
    this.timer = setInterval(() => void this.refresh(), 5000);
    void this.refresh();
  }
  async refresh() {
    if (!this.alive || this.polling) return;
    this.polling = true;
    const request = ++this.request;
    try {
      const next = await recoveryStatus();
      if (this.alive && request === this.request) this.status = next;
    } catch (error) {
      if (this.alive && request === this.request)
        this.status.error = String(error);
    } finally {
      this.polling = false;
    }
  }
  async choose(id: string, recover: boolean) {
    if (!this.alive || !this.workspace.canAct) return;
    this.active = true;
    this.workspace.error = '';
    try {
      if (recover) {
        if (!(await this.effects.protect()) || !this.alive) return;
        await this.subtitles.command({ kind: 'recover', id, discard: true });
        if (!this.alive) return;
        this.effects.suspendFollowing();
        this.workspace.notice =
          'Recovered as an unsaved copy. Save As chooses its destination; the recovery snapshot is retained.';
      } else {
        ++this.request;
        await discardRecovery(id);
        if (!this.alive) return;
        // A poll admitted during removal also predates confirmed deletion.
        ++this.request;
        this.status.entries = this.status.entries.filter(
          (entry) => entry.id !== id,
        );
        this.discardId = null;
      }
    } catch (error) {
      if (this.alive) this.workspace.error = String(error);
    } finally {
      if (this.alive) this.active = false;
    }
  }
  dispose() {
    this.alive = false;
    ++this.request;
    if (this.timer !== null) clearInterval(this.timer);
    this.timer = null;
  }
}
