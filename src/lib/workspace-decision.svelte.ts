import { PendingDecision } from './pending-decision';

interface DecisionEffects {
  occlude: (value: boolean) => Promise<void>;
  remainingOcclusion: () => boolean;
  close: () => void;
  afterRender: () => Promise<void>;
  reportError: (error: unknown) => void;
}

// Presentation and one answer, not save policy. App derives confirming from active.
export class WorkspaceDecision<Choice> {
  private phase = $state<'idle' | 'opening' | 'open' | 'closing'>('idle');
  private result: Promise<Choice> | null = null;
  private alive = true;
  private epoch = 0;
  private answering = false;
  private decision: PendingDecision<Choice>;

  constructor(
    private cancelled: Choice,
    private effects: DecisionEffects,
  ) {
    this.decision = new PendingDecision(cancelled);
  }

  get active() {
    return this.phase === 'opening' || this.phase === 'open';
  }
  get visible() {
    return this.phase === 'open';
  }
  get closing() {
    return this.phase === 'closing';
  }

  async ask(present: () => void): Promise<Choice> {
    return (await this.open(present)) ? this.wait() : this.cancelled;
  }

  // Import releases App's action gate once shown, then answers on dialog close.
  // Save decisions instead use ask() to await their protected continuation.
  async open(present: () => void): Promise<boolean> {
    if (!this.alive) return false;
    if (this.phase !== 'idle' || this.decision.pending)
      throw new Error('A dialog decision is already pending.');
    const epoch = ++this.epoch;
    this.phase = 'opening';
    try {
      await this.effects.occlude(true);
      if (!this.current(epoch)) return false;
      present();
      this.result = this.decision.wait();
      this.phase = 'open';
      return true;
    } catch (error) {
      if (this.current(epoch)) {
        this.phase = 'idle';
        throw error;
      }
      return false;
    }
  }

  wait(): Promise<Choice> {
    return this.result ?? Promise.resolve(this.cancelled);
  }

  async answer(choice: Choice) {
    if (!this.alive || !this.decision.pending || this.answering) return;
    const epoch = this.epoch;
    this.answering = true;
    this.effects.close();
    this.phase = 'closing';
    try {
      await this.effects.afterRender();
      if (!this.current(epoch)) return;
      await this.effects.occlude(this.effects.remainingOcclusion());
      if (this.current(epoch)) {
        this.phase = 'idle';
        this.decision.answer(choice);
      }
    } catch (error) {
      if (this.current(epoch)) {
        this.phase = 'idle';
        this.effects.reportError(error);
        this.decision.answer(this.cancelled);
      }
    } finally {
      if (this.current(epoch)) this.answering = false;
    }
  }

  private current(epoch: number) {
    return this.alive && epoch === this.epoch;
  }

  dispose() {
    this.alive = false;
    ++this.epoch;
    this.phase = 'idle';
    this.decision.dispose();
  }
}
