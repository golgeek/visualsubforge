// Only the newest pending preview is useful. Serialize commands so an older
// response cannot overwrite a later boundary or race a clear/open operation.
export class LiveLoop {
  private session: number | null = null;
  private pending: { session: number; range: [number, number] } | null = null;
  private running: Promise<void> | null = null;
  private send: (session: number, range: [number, number]) => Promise<void>;
  private failed: (error: unknown) => void;
  constructor(
    send: (session: number, range: [number, number]) => Promise<void>,
    failed: (error: unknown) => void,
  ) {
    this.send = send;
    this.failed = failed;
  }
  activate(session: number) {
    this.session = session;
  }
  update(session: number, range: [number, number]) {
    if (session !== this.session) return;
    this.pending = { session, range: [...range] };
    if (!this.running) {
      this.running = this.drain().finally(() => {
        this.running = null;
        const next = this.pending;
        if (next) this.update(next.session, next.range);
      });
    }
  }
  private async drain() {
    while (this.pending) {
      const next = this.pending;
      this.pending = null;
      try {
        await this.send(next.session, next.range);
      } catch (error) {
        if (this.session === next.session) this.failed(error);
      }
    }
  }
  async flush() {
    while (this.running) await this.running;
  }
  async stop() {
    this.session = null;
    this.pending = null;
    await this.flush();
  }
}
