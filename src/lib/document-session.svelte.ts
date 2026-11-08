import {
  DocumentFailure,
  chooseFile,
  documentAction,
  resyncAccept,
  gapAccept,
  sceneRepairAccept,
  durationRepairAccept,
  batchTimingAccept,
  textAccept,
  styleAccept,
  fontAccept,
  pluginAccept,
  type PluginInput,
  translationAction,
  type TranslationAction,
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
import { retryEdit, type CueEdit } from './edit-retry';
import {
  selectCues,
  selectAllCues,
  reconcileSelection,
  type CueSelection,
  type SelectionMode,
} from './cue-selection';

const client = {
  chooseFile,
  documentAction,
  resyncAccept,
  gapAccept,
  sceneRepairAccept,
  durationRepairAccept,
  batchTimingAccept,
  textAccept,
  styleAccept,
  fontAccept,
  pluginAccept,
  translationAction,
};
type Client = Omit<
  typeof client,
  | 'styleAccept'
  | 'fontAccept'
  | 'pluginAccept'
  | 'textAccept'
  | 'sceneRepairAccept'
  | 'durationRepairAccept'
  | 'batchTimingAccept'
  | 'translationAction'
> &
  Partial<
    Pick<
      typeof client,
      | 'styleAccept'
      | 'fontAccept'
      | 'pluginAccept'
      | 'textAccept'
      | 'sceneRepairAccept'
      | 'durationRepairAccept'
      | 'batchTimingAccept'
      | 'translationAction'
    >
  >;
type Version = Pick<DocumentSnapshot, 'generation' | 'revision'>;
type CueDraft = { text: string; start: string; end: string; style: string };
type CommandOptions = {
  plan?: CueOperationPlan;
  version?: Version;
  preferred?: number | null;
  preserveDraft?: boolean;
  submittedDraft?: CueDraft;
};

// Rust owns the document and history. This owner replaces accepted projections;
// Only selection and cue draft fields are editable frontend state.
export class DocumentSession {
  get styled() {
    return ['ass', 'ssa'].includes(this.accepted.format?.name ?? '');
  }
  private accepted = $state.raw<DocumentSnapshot>({ ...emptyDocument });
  private selection = $state<number | null>(null);
  private multi = $state.raw<CueSelection>(selectAllCues([], null));
  private alive = true;
  private pending = false;
  private client: Client;
  text = $state('');
  start = $state('');
  end = $state('');
  style = $state('');
  conflict = $state('');

  constructor(api: Client = client) {
    this.client = api;
  }
  get snapshot() {
    return this.accepted;
  }
  get selected() {
    return this.selection;
  }
  get selectedIds() {
    return this.multi.ids;
  }
  get cue() {
    return this.accepted.cues.find((cue) => cue.id === this.selection);
  }
  get dirty() {
    if (this.conflict) return true;
    const cue = this.cue;
    return (
      !!cue &&
      (this.text !== cue.text ||
        this.start !== formatTime(cue.startMs) ||
        this.end !== formatTime(cue.endMs) ||
        this.style !== (cue.style ?? ''))
    );
  }
  select(id: number | null, mode: SelectionMode = 'replace') {
    if (!this.alive || this.conflict) return;
    this.multi = selectCues(this.accepted.cues, this.multi, id, mode);
    this.selection = this.multi.primary;
    this.revert();
  }
  selectAll() {
    if (!this.alive || this.conflict) return;
    this.selection = this.selection ?? this.accepted.cues[0]?.id ?? null;
    this.multi = selectAllCues(this.accepted.cues, this.selection);
    this.revert();
  }
  revert() {
    if (!this.alive) return;
    this.conflict = '';
    const cue = this.cue;
    this.text = cue?.text ?? '';
    this.start = cue ? formatTime(cue.startMs) : '';
    this.end = cue ? formatTime(cue.endMs) : '';
    this.style = cue?.style ?? '';
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
  async changeTranslation(
    action: Extract<
      TranslationAction,
      { kind: 'accept' | 'setOffset' | 'detach' }
    >,
    expected: DocumentSnapshot,
  ): Promise<boolean> {
    if (this.accepted !== expected)
      return Promise.reject(
        new Error('The target or reference changed; try again.'),
      );
    try {
      return await this.update(
        async (before) => {
          const reply = await (
            this.client.translationAction ?? translationAction
          )(action);
          if (reply.document) return reply.document;
          if (
            reply.key.generation !== before.generation ||
            reply.key.revision !== before.revision
          )
            throw new Error(
              'The target changed during the reference operation; reopen its current state.',
            );
          return { ...before, translation: reply.context };
        },
        { preserveDraft: true },
      );
    } catch (failure) {
      // A lost acceptance reply can follow a successful Rust commit. Refresh
      // accepted state, retaining the draft when the target version is unchanged.
      if (this.alive && this.accepted === expected)
        await this.command({ kind: 'status' }, { preserveDraft: true }).catch(
          () => {},
        );
      throw failure;
    }
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
  acceptStyle(id: number, expected: DocumentSnapshot): Promise<boolean> {
    if (this.accepted !== expected || this.dirty)
      return Promise.reject(
        new Error('Subtitles or the cue draft changed; preview again.'),
      );
    return this.update(() => (this.client.styleAccept ?? styleAccept)(id));
  }
  acceptFont(id: number, expected: DocumentSnapshot): Promise<boolean> {
    if (this.accepted !== expected || this.dirty)
      return Promise.reject(
        new Error('Subtitles or the cue draft changed; preview again.'),
      );
    return this.update(() => (this.client.fontAccept ?? fontAccept)(id));
  }
  acceptPlugin(
    id: number,
    expected: DocumentSnapshot,
    input: PluginInput,
  ): Promise<boolean> {
    if (this.accepted !== expected || this.dirty)
      return Promise.reject(
        new Error('Subtitles or the cue draft changed; run the plugin again.'),
      );
    return this.update(() =>
      (this.client.pluginAccept ?? pluginAccept)(id, input),
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
  acceptBatchTiming(id: number, expected: DocumentSnapshot): Promise<boolean> {
    if (this.accepted !== expected || this.dirty)
      return Promise.reject(
        new Error('Subtitles or the cue draft changed; preview again.'),
      );
    return this.update(() =>
      (this.client.batchTimingAccept ?? batchTimingAccept)(id),
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
      const currentDraft = this.captureDraft();
      const changedDraft =
        options.submittedDraft &&
        (Object.keys(currentDraft) as (keyof CueDraft)[]).some(
          (key) => currentDraft[key] !== options.submittedDraft![key],
        );
      this.adopt(next, before, options);
      if (changedDraft) {
        // A successful commit is authoritative, but cannot consume newer input
        // or authorize a Save/navigation continuation for that unapplied draft.
        Object.assign(this, currentDraft);
        throw new Error(
          'The earlier edit was accepted; newer draft input remains unapplied.',
        );
      }
      return true;
    } finally {
      this.pending = false;
    }
  }
  private adopt(
    next: DocumentSnapshot,
    before: DocumentSnapshot,
    options: CommandOptions = {},
  ) {
    const preferred = options.plan
      ? operationSelection(options.plan, before.cues, next.cues)
      : options.preferred;
    this.selection =
      next.generation !== before.generation
        ? (next.cues[0]?.id ?? null)
        : preferred === undefined
          ? this.selection === null
            ? null
            : reconcileCueSelection(before.cues, next.cues, this.selection)
          : preferred;
    this.multi =
      next.generation !== before.generation
        ? selectCues(next.cues, this.multi, this.selection, 'replace')
        : reconcileSelection(next.cues, this.multi, this.selection);
    this.accepted = next;
    if (
      !options.preserveDraft ||
      next.generation !== before.generation ||
      next.revision !== before.revision
    )
      this.revert();
  }

  private captureDraft(): CueDraft {
    return {
      text: this.text,
      start: this.start,
      end: this.end,
      style: this.style,
    };
  }

  private retainConflict(next: DocumentSnapshot | null, message: string) {
    const draft = this.captureDraft();
    if (next) this.adopt(next, this.accepted);
    Object.assign(this, draft);
    this.conflict = message;
  }

  private async submitEdit(
    edit: CueEdit,
    base: DocumentSnapshot,
  ): Promise<DocumentSnapshot> {
    const draft = [this.text, this.start, this.end, this.style];
    const selected = this.selection;
    try {
      return await this.client.documentAction(edit, base);
    } catch (failure) {
      if (!this.alive) throw failure;
      // A structured rejection proves no commit occurred. Other failures may be
      // lost replies: reconcile, but never replay an unknown-outcome mutation.
      const kind =
        failure instanceof DocumentFailure ? failure.detail.kind : null;
      if (kind === 'rejected') throw failure;
      let current: DocumentSnapshot;
      try {
        current = await this.client.documentAction({ kind: 'status' }, base);
      } catch {
        if (this.alive)
          this.retainConflict(
            null,
            'Could not refresh subtitle state. Draft retained; reopen authoritative state before resolving it.',
          );
        throw failure;
      }
      if (!this.alive) throw failure;
      const retry =
        kind === 'staleRevision' &&
        this.selection === selected &&
        this.accepted === base &&
        draft.every(
          (value, i) =>
            value === [this.text, this.start, this.end, this.style][i],
        )
          ? retryEdit(base, current, edit)
          : null;
      if (retry) {
        try {
          return await this.client.documentAction(retry, current);
        } catch (second) {
          if (!this.alive) throw second;
          // No third attempt, even for another confirmed stale revision. Read
          // again because the retry itself may have committed before a lost ACK.
          let latest: DocumentSnapshot | null = null;
          try {
            latest = await this.client.documentAction(
              { kind: 'status' },
              current,
            );
          } catch {
            /* Retain the draft and block further application. */
          }
          if (this.alive)
            this.retainConflict(
              latest ?? current,
              'The edit could not be confirmed after retry. Draft retained; copy it if needed, then Revert draft to use current subtitles.',
            );
          throw second;
        }
      }
      this.retainConflict(
        current,
        'Subtitles changed or the edit outcome is uncertain. Draft retained; copy it if needed, then Revert draft to use current subtitles.',
      );
      throw failure;
    }
  }
  async apply(): Promise<boolean> {
    if (!this.alive) return false;
    if (this.conflict) throw new Error(this.conflict);
    const cue = this.cue;
    if (!cue || !this.dirty) return true;
    const fields = {
      cueId: cue.id,
      startMs: parseTime(this.start),
      endMs: parseTime(this.end),
      text: this.text,
    };
    const edit: CueEdit = this.styled
      ? { kind: 'editStyled', ...fields, style: this.style }
      : { kind: 'edit', ...fields };
    return this.update((base) => this.submitEdit(edit, base), {
      submittedDraft: this.captureDraft(),
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
