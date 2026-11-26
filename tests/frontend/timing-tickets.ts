// Model the native per-proposal latch, including release before registration.
export class TimingTickets {
  private next = 0;
  private tickets = new Map<
    number,
    { owner: number; done: Promise<void>; resolve: () => void }
  >();
  private retired = new Set<number>();

  register(owner: number) {
    if (this.retired.has(owner)) return null;
    const ticket = ++this.next;
    let resolve!: () => void;
    const done = new Promise<void>((r) => {
      resolve = r;
    });
    this.tickets.set(ticket, { owner, done, resolve });
    return ticket;
  }
  async wait(ticket: number) {
    await this.tickets.get(ticket)?.done;
    this.withdraw(ticket);
  }
  withdraw(ticket: number) {
    this.tickets.get(ticket)?.resolve();
    this.tickets.delete(ticket);
  }
  release(owner: number) {
    this.retired.add(owner);
    for (const ticket of this.tickets.values())
      if (ticket.owner === owner) ticket.resolve();
  }
}
