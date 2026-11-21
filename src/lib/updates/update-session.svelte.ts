import {
  checkUpdates,
  setUpdateConsent,
  dismissUpdate,
  openUpdate,
  type UpdateOutcome,
} from '../client';

export class UpdateSession {
  result = $state<UpdateOutcome | null>(null);
  checking = $state(false);
  saving = $state(false);
  postponed = $state(false);
  error = $state('');
  private generation = 0;
  private alive = true;

  constructor(
    private acceptConsent: (enabled: boolean) => void,
    private automaticEnabled: () => boolean,
  ) {}

  async consent(enabled: boolean) {
    if (this.saving) return;
    this.saving = true;
    this.error = '';
    try {
      await setUpdateConsent(enabled);
      if (this.alive) this.acceptConsent(enabled);
    } catch (error) {
      if (this.alive) this.error = String(error);
    } finally {
      if (this.alive) this.saving = false;
    }
  }

  async check(manual: boolean) {
    if (!manual && (this.checking || this.result || !this.automaticEnabled()))
      return;
    const generation = ++this.generation;
    this.checking = manual;
    this.error = '';
    if (manual) this.result = null;
    try {
      const result = await checkUpdates(manual);
      if (
        generation === this.generation &&
        (manual || this.automaticEnabled()) &&
        result.kind !== 'quiet'
      )
        this.result = result;
    } catch {
      if (generation === this.generation && manual)
        this.result = { kind: 'failed', reason: 'unavailable' };
    } finally {
      if (generation === this.generation) this.checking = false;
    }
  }

  async dismiss() {
    const current = this.result;
    try {
      if (current?.kind === 'available') await dismissUpdate(current.version);
      if (this.alive && this.result === current) this.result = null;
    } catch (error) {
      if (this.alive) this.error = String(error);
    }
  }

  async view() {
    if (this.result?.kind !== 'available') return;
    try {
      await openUpdate(this.result.version);
    } catch (error) {
      if (this.alive) this.error = String(error);
    }
  }

  dispose() {
    this.alive = false;
    ++this.generation;
  }
}
