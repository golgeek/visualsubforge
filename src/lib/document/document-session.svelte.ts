import {
  DocumentFailure,
  chooseFile,
  documentAction,
  resyncAccept,
  gapAccept,
  sceneRepairAccept,
  durationRepairAccept,
  cueFixesAccept,
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
} from '../client';
import {
  operationSelection,
  reconcileCueSelection,
  type CueOperationPlan,
} from '../cues/cue-operations';
import { formatTime, parseTime } from '../timeline/timeline';
import {
  selectCues,
  selectAllCues,
  reconcileSelection,
  type CueSelection,
  type SelectionMode,
} from '../cues/cue-selection';

const client = {
  chooseFile,
  documentAction,
  resyncAccept,
  gapAccept,
  sceneRepairAccept,
  durationRepairAccept,
  cueFixesAccept,
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
  | 'cueFixesAccept'
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
      | 'cueFixesAccept'
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
  retainNewerDraft?: boolean;
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
  private pendingEdit = $state.raw<Promise<boolean> | null>(null);
  private commitSession: () => string | null = () => null;
  private client: Client;
  editError = $state('');
  stateConfirmed = $state(true);
  text = $state('');
  start = $state('');
  end = $state('');
  style = $state('');

  constructor(api: Client = client) {
    this.client = api;
  }
  connectCommitSession(read: () => string | null) {
    if (this.alive) this.commitSession = read;
  }
  get committing() {
    return this.pendingEdit !== null;
  }
  async waitForCommit(): Promise<boolean> {
    return this.pendingEdit ? await this.pendingEdit : this.alive;
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
    const cue = this.cue;
    if (!cue)
      return Object.values(this.captureDraft()).some((value) => value !== '');
    return (
      !!cue &&
      (this.text !== cue.text ||
        this.start !== formatTime(cue.startMs) ||
        this.end !== formatTime(cue.endMs) ||
        this.style !== (cue.style ?? ''))
    );
  }
  select(
    id: number | null,
    mode: SelectionMode = 'replace',
    order: readonly { id: number }[] = this.accepted.cues,
  ) {
    if (!this.alive) return;
    this.multi = selectCues(order, this.multi, id, mode);
    this.selection = this.multi.primary;
    this.revert();
  }
  selectAll(order: readonly { id: number }[] = this.accepted.cues) {
    if (!this.alive) return;
    this.selection = order.some((c) => c.id === this.selection)
      ? this.selection
      : (order[0]?.id ?? null);
    this.multi = selectAllCues(order, this.selection);
    this.revert();
  }
  revert() {
    if (!this.alive) return;
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
      action.kind === 'status',
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
  async acceptCueFixes(
    id: number,
    expected: DocumentSnapshot,
  ): Promise<boolean> {
    if (this.accepted !== expected || this.dirty || !this.stateConfirmed)
      throw new Error('Subtitles or the cue edit changed; check fixes again.');
    const selected = this.selection;
    let dispatched = false;
    try {
      return await this.update(() => {
        dispatched = true;
        return (this.client.cueFixesAccept ?? cueFixesAccept)(id);
      });
    } catch (failure) {
      if (dispatched && this.alive && this.accepted === expected) {
        this.stateConfirmed = false;
        await this.refreshExclusively(expected, selected, false).catch(
          () => {},
        );
        if (this.alive && !this.stateConfirmed)
          this.editError =
            'Could not confirm cue fixes. Retry edit to refresh before continuing.';
      }
      throw failure;
    }
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
    readOnly = false,
  ): Promise<boolean> {
    if (!this.alive) return false;
    if (!this.stateConfirmed && !readOnly)
      throw new Error('Retry edit to refresh subtitles before continuing.');
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
        if (options.retainNewerDraft) return true;
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
    this.stateConfirmed = true;
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

  private async submitEdit(
    edit: Extract<DocumentAction, { kind: 'edit' | 'editStyled' }>,
    base: DocumentSnapshot,
    textSession: string | null,
  ): Promise<DocumentSnapshot> {
    const selected = this.selection;
    try {
      return await this.client.documentAction(edit, base, textSession);
    } catch (failure) {
      if (!this.alive) throw failure;
      if (
        failure instanceof DocumentFailure &&
        failure.detail.kind === 'rejected'
      )
        throw failure;
      const message =
        failure instanceof Error ? failure.message : String(failure);
      this.stateConfirmed = false;
      // Stale refusal and unknown outcome both require authoritative state.
      // Never replay: a lost reply can follow a successful Rust commit.
      try {
        if (!(await this.refreshEditBase(base, selected))) throw failure;
      } catch {
        if (!this.alive) throw failure;
        throw new Error(
          `${message} — Could not refresh subtitles; draft retained.`,
          { cause: failure },
        );
      }
      throw new Error(
        `${message} — Current subtitles refreshed; draft retained. Review it before applying again.`,
        { cause: failure },
      );
    }
  }
  private async refreshEditBase(
    base: DocumentSnapshot,
    selected: number | null,
    retainCleanDraft = true,
  ) {
    const current = await this.client.documentAction({ kind: 'status' }, base);
    if (!this.alive) return false;
    const draft = this.captureDraft();
    const keep = retainCleanDraft || this.dirty;
    this.adopt(current, base);
    if (
      current.generation !== base.generation ||
      !current.cues.some((c) => c.id === selected)
    ) {
      this.selection = null;
      this.multi = selectCues(current.cues, this.multi, null, 'replace');
    }
    if (keep) Object.assign(this, draft);
    return true;
  }
  private async refreshExclusively(
    base: DocumentSnapshot,
    selected: number | null,
    retainCleanDraft = true,
  ): Promise<boolean> {
    if (!this.alive) return false;
    if (this.pending)
      throw new Error('Wait for the current subtitle operation.');
    this.pending = true;
    try {
      return await this.refreshEditBase(base, selected, retainCleanDraft);
    } finally {
      this.pending = false;
    }
  }
  private async refreshUnknownEdit(): Promise<boolean> {
    try {
      if (!(await this.refreshExclusively(this.accepted, this.selection)))
        return false;
      this.editError =
        'Current subtitles refreshed. Review the retained edit, then retry.';
    } catch (failure) {
      if (!this.alive) return false;
      this.editError = `Could not refresh subtitles; edit retained. ${String(failure)}`;
    }
    throw new Error(this.editError);
  }
  async apply(textSession = this.commitSession()): Promise<boolean> {
    // Explicit continuations wait for an automatic commit, then submit any
    // newer fields once. A rejection propagates; unknown outcomes are not replayed.
    while (this.pendingEdit) {
      if (!(await this.pendingEdit)) return false;
    }
    if (!this.stateConfirmed) {
      const request = this.refreshUnknownEdit().finally(() => {
        if (this.pendingEdit === request) this.pendingEdit = null;
      });
      this.pendingEdit = request;
      return request;
    }
    return this.commitDraft(textSession, false);
  }
  async commitAutomatically(textSession: string | null): Promise<boolean> {
    if (this.pendingEdit) return this.pendingEdit;
    if (!this.stateConfirmed) return false;
    return this.commitDraft(textSession, true);
  }
  private async commitDraft(
    textSession: string | null,
    retainNewerDraft: boolean,
  ): Promise<boolean> {
    try {
      if (!this.alive) return false;
      const cue = this.cue;
      if (!cue) return !this.dirty;
      if (!this.dirty) return true;
      const fields = {
        cueId: cue.id,
        startMs: parseTime(this.start),
        endMs: parseTime(this.end),
        text: this.text,
      };
      const edit: Extract<DocumentAction, { kind: 'edit' | 'editStyled' }> =
        this.styled
          ? { kind: 'editStyled', ...fields, style: this.style }
          : { kind: 'edit', ...fields };
      this.editError = '';
      const request = this.update(
        (base) => this.submitEdit(edit, base, textSession),
        {
          submittedDraft: this.captureDraft(),
          retainNewerDraft,
        },
      ).finally(() => {
        if (this.pendingEdit === request) this.pendingEdit = null;
      });
      this.pendingEdit = request;
      return await request;
    } catch (failure) {
      if (this.alive)
        this.editError =
          failure instanceof Error ? failure.message : String(failure);
      throw failure;
    }
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
