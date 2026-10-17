import {
  chooseFile,
  documentAction,
  resyncAccept,
  gapAccept,
  sceneRepairAccept,
  durationRepairAccept,
  textAccept,
  type TextTarget,
  emptyDocument,
  type DocumentAction,
  type DocumentSnapshot,
} from './client';
import {
  operationSelection,
  reconcileCueSelection,
  type CueOperationPlan,
} from './cue-operations';
import { formatTime, parseTime } from './timeline';

const client = {
  chooseFile,
  documentAction,
  resyncAccept,
  gapAccept,
  sceneRepairAccept,
  durationRepairAccept,
  textAccept,
};
type Client = Omit<
  typeof client,
  'textAccept' | 'sceneRepairAccept' | 'durationRepairAccept'
> &
  Partial<
    Pick<
      typeof client,
      'textAccept' | 'sceneRepairAccept' | 'durationRepairAccept'
    >
  >;
type Version = Pick<DocumentSnapshot, 'generation' | 'revision'>;
type CommandOptions = {
  plan?: CueOperationPlan;
  version?: Version;
  preferred?: number | null;
};

// Rust owns the document and history. This owner replaces accepted projections;
// only selection and the three draft fields are editable frontend state.
export class DocumentSession {
  private accepted = $state.raw<DocumentSnapshot>({ ...emptyDocument });
  private selection = $state<number | null>(null);
  private alive = true;
  private pending = false;
  private client: Client;
  text = $state('');
  start = $state('');
  end = $state('');

  constructor(api: Client = client) {
    this.client = api;
  }
  get snapshot() {
    return this.accepted;
  }
  get selected() {
    return this.selection;
  }
  get cue() {
    return this.accepted.cues.find((cue) => cue.id === this.selection);
  }
  get dirty() {
    const cue = this.cue;
    return (
      !!cue &&
      (this.text !== cue.text ||
        this.start !== formatTime(cue.startMs) ||
        this.end !== formatTime(cue.endMs))
    );
  }
  select(id: number | null) {
    if (!this.alive) return;
    this.selection = this.accepted.cues.some((cue) => cue.id === id)
      ? id
      : null;
    this.revert();
  }
  revert() {
    if (!this.alive) return;
    const cue = this.cue;
    this.text = cue?.text ?? '';
    this.start = cue ? formatTime(cue.startMs) : '';
    this.end = cue ? formatTime(cue.endMs) : '';
  }
  command(
    action: DocumentAction,
    options: CommandOptions = {},
  ): Promise<boolean> {
    return this.update(
      (before) => this.client.documentAction(action, options.version ?? before),
      options,
    );
  }
  acceptResync(id: number, expected: DocumentSnapshot): Promise<boolean> {
    if (this.accepted !== expected || this.dirty)
      return Promise.reject(
        new Error('Subtitles or the cue draft changed; analyze again.'),
      );
    return this.update(() => this.client.resyncAccept(id));
  }
  acceptGap(
    id: number,
    expected: DocumentSnapshot,
    acknowledgeNoMedia: boolean,
  ): Promise<boolean> {
    if (this.accepted !== expected || this.dirty)
      return Promise.reject(
        new Error('Subtitles or the cue draft changed; preview again.'),
      );
    return this.update(() => this.client.gapAccept(id, acknowledgeNoMedia));
  }
  acceptText(
    id: number,
    expected: DocumentSnapshot,
    target: TextTarget,
  ): Promise<boolean> {
    if (this.accepted !== expected || this.dirty)
      return Promise.reject(
        new Error('Subtitles or the cue draft changed; preview again.'),
      );
    return this.update(() =>
      (this.client.textAccept ?? textAccept)(id, target),
    );
  }
  acceptSceneRepair(id: number, expected: DocumentSnapshot): Promise<boolean> {
    if (this.accepted !== expected || this.dirty)
      return Promise.reject(
        new Error('Subtitles or the cue draft changed; preview again.'),
      );
    return this.update(() =>
      (this.client.sceneRepairAccept ?? sceneRepairAccept)(id),
    );
  }
  acceptDurationRepair(
    id: number,
    expected: DocumentSnapshot,
  ): Promise<boolean> {
    if (this.accepted !== expected || this.dirty)
      return Promise.reject(
        new Error('Subtitles or the cue draft changed; preview again.'),
      );
    return this.update(() =>
      (this.client.durationRepairAccept ?? durationRepairAccept)(id),
    );
  }
  private async update(
    request: (before: DocumentSnapshot) => Promise<DocumentSnapshot>,
    options: CommandOptions = {},
  ): Promise<boolean> {
    if (!this.alive) return false;
    // App disables editing while a command is pending. Enforce the same rule
    // here so a second caller cannot send a mutation with the same revision.
    if (this.pending)
      throw new Error('Wait for the current subtitle operation.');
    this.pending = true;
    const before = this.accepted;
    try {
      const next = await request(before);
      if (!this.alive) return false;
      const preferred = options.plan
        ? operationSelection(options.plan, before.cues, next.cues)
        : options.preferred;
      this.selection =
        next.generation !== before.generation
          ? (next.cues[0]?.id ?? null)
          : preferred === undefined
            ? reconcileCueSelection(before.cues, next.cues, this.selection)
            : preferred;
      this.accepted = next;
      this.revert();
      return true;
    } finally {
      this.pending = false;
    }
  }
  async apply(): Promise<boolean> {
    if (!this.alive) return false;
    const cue = this.cue;
    if (!cue || !this.dirty) return true;
    return this.command({
      kind: 'edit',
      cueId: cue.id,
      startMs: parseTime(this.start),
      endMs: parseTime(this.end),
      text: this.text,
    });
  }
  async save(as: boolean): Promise<boolean> {
    if (!(await this.apply())) return false;
    as = as || this.accepted.requiresSaveAs;
    const path = as
      ? await this.client.chooseFile('saveSubtitles', this.accepted.path)
      : null;
    if (!this.alive || (as && !path)) return false;
    return this.command({ kind: 'save', path });
  }
  dispose() {
    // An already accepted Rust edit is not rolled back. Ignore its late reply
    // and prevent subsequent chooser/save operations from this dead workspace.
    this.alive = false;
  }
}
