import {
  type CommentEdit,
  type ReviewThread,
  chooseFile,
  reviewAction,
  type ReviewChange,
  type ReviewReply,
} from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
import type { UnsavedChanges } from '../document/unsaved-changes';
import type { Workspace } from '../workspace/workspace.svelte';
import type { WorkspaceView } from '../workspace/workspace-view.svelte';
export type ExchangePrompt =
  | { kind: 'package' | 'report'; dirty: boolean; author: string }
  | { kind: 'different'; author: string; time: number };
export type ExchangeAnswer =
  | 'theirs'
  | 'mine'
  | 'cancel'
  | { kind: 'export' | 'readable'; author: string };
export type BlockRow = NonNullable<ReviewReply['blocks']>[number];
export class ReviewSession {
  page = $state.raw<ReviewReply | null>(null);
  expanded = $state.raw<number[]>([]);
  private expandedCapture = '';
  offset = $state(0);
  filter = $state<ReviewReply['rows'][number]['kind'] | null>(null);
  private selectedCue: number | null = null;
  selected = $state<string | null>(null);
  error = $state('');
  projection = $state.raw<ReviewChange[]>([]);
  showOriginal = $state(true);
  includeUnchanged = $state(false);
  threadFilter = $state<'open' | 'commented' | null>(null);
  visibleDiscussion = $state.raw<ReviewReply['discussion']>(undefined);
  currentThread = $state<string | null>(null);
  name: () => string = () => '';
  saveName: (name: string) => Promise<void> = async () => {};
  goToTime: (ms: number) => void = () => {};
  rowId(c: ReviewChange) {
    return c.key + '/' + (c.original[0] ?? c.revised[0])?.source_index;
  }
  threads(c: ReviewChange, projection = false) {
    return (
      (projection ? this.visibleDiscussion : this.page?.discussion)?.rows[
        this.rowId(c)
      ] ?? []
    );
  }
  async comment(edit: CommentEdit, inlineName: string) {
    if (!this.workspace.canAct) return false;
    const key = this.key();
    let posted = false;
    await this.workspace.task(async () => {
      if (!this.name() && edit.kind !== 'delete')
        await this.saveName(inlineName.trim());
      if (!this.alive || JSON.stringify(key) !== JSON.stringify(this.key()))
        return;
      posted = await this.subtitles.changeReview(
        { kind: 'comment', key, author: this.name(), atMs: Date.now(), edit },
        this.subtitles.snapshot,
      );
    });
    return posted;
  }
  async navigateThread(forward: boolean) {
    if (!this.workspace.canAct || !this.page?.statistics) return;
    const key = this.key();
    try {
      const r = await reviewAction({
        kind: 'navigateThread',
        key,
        from: this.currentThread,
        forward,
      });
      if (
        !this.alive ||
        JSON.stringify(key) !== JSON.stringify(this.key()) ||
        !r.threadId
      )
        return;
      this.view.openReview('changes');
      this.currentThread = r.threadId;
      this.threadFilter = 'open';
      this.offset = Math.floor(((r.position ?? 1) - 1) / 100) * 100;
      if (r.rows[0]) await this.select(r.rows[0]);
      else if (r.discussion?.detached[0])
        this.frameThread(r.discussion.detached[0]);
    } catch (error) {
      if (this.alive) this.error = String(error);
    }
  }
  frameThread(t: ReviewThread) {
    const q =
      t.anchor.revised[0] ??
      (t.anchor.kind === 'change' ? t.anchor.original[0] : null);
    if (q) this.goToTime(q.start_ms!);
  }
  navigateTo: (change: ReviewChange, block?: BlockRow) => Promise<void> =
    async () => {};
  projectionInput: () => { ids: number[]; startMs: number; endMs: number } =
    () => ({ ids: [], startMs: 0, endMs: 0 });
  private projectionSequence = 0;
  get options() {
    return this.subtitles.snapshot.review?.options;
  }
  get canMutate() {
    return (
      this.workspace.canAct && !this.subtitles.dirty && !!this.page?.statistics
    );
  }
  get selectedChange() {
    return (
      this.projection.find((c) =>
        c.revised.some((cue) => cue.cue_id === this.subtitles.selected),
      ) ?? null
    );
  }

  detachProject: () => Promise<void> = async () => {};
  projectAttached: () => boolean = () => false;
  decideProject: () => Promise<'save' | 'later'> = async () => 'later';
  private offeredProject = '';
  async afterSave() {
    const context = this.subtitles.snapshot.review;
    const identity = `${this.subtitles.snapshot.generation}/${context?.path}`;
    if (
      !context?.path ||
      this.projectAttached() ||
      this.offeredProject === identity
    )
      return;
    this.offeredProject = identity;
    if ((await this.decideProject()) === 'save' && this.alive)
      await this.saveProject();
  }
  private alive = true;
  private sequence = 0;
  constructor(
    private subtitles: DocumentSession,
    private workspace: Workspace,
    private view: WorkspaceView,
    private unsaved: UnsavedChanges,
    private saveProject: () => Promise<boolean>,
    private decideSwap: (path: string) => Promise<'swap' | 'keep' | 'cancel'>,
  ) {
    $effect(() => {
      const snapshot = this.subtitles.snapshot;
      const capture = `${snapshot.generation}/${snapshot.revision}/${snapshot.review?.epoch}`;
      if (this.expandedCapture !== capture) {
        this.expandedCapture = capture;
        this.expanded = [];
      }
      const expanded = this.expanded;
      const offset = this.offset;
      const filter = this.filter;
      const threadFilter = this.threadFilter;
      const sequence = ++this.sequence;
      this.page = null;
      this.projection = [];
      this.visibleDiscussion = undefined;
      if (!snapshot.review?.path) return;
      const timer = setTimeout(
        () =>
          void this.query(
            capture,
            offset,
            filter,
            sequence,
            expanded,
            threadFilter,
          ),
        250,
      );
      return () => {
        clearTimeout(timer);
      };
    });
  }
  async projectVisible() {
    const input = this.projectionInput();
    const key = this.key();
    const serial = ++this.projectionSequence;
    try {
      const reply = await reviewAction({
        kind: 'projection',
        key,
        ids: input.ids.slice(0, 200),
        startMs: Math.max(0, Math.round(input.startMs)),
        endMs: Math.max(0, Math.round(input.endMs)),
      });
      if (
        this.alive &&
        serial === this.projectionSequence &&
        JSON.stringify(key) === JSON.stringify(this.key())
      ) {
        this.projection = reply.rows;
        this.visibleDiscussion = reply.discussion;
      }
    } catch (error) {
      if (
        this.alive &&
        serial === this.projectionSequence &&
        JSON.stringify(key) === JSON.stringify(this.key())
      )
        this.error = String(error);
    }
  }
  async select(change: ReviewChange, block?: BlockRow) {
    if (!this.workspace.canAct) return;
    const key = this.key();
    await this.navigateTo(change, block);
    if (this.alive && JSON.stringify(key) === JSON.stringify(this.key())) {
      this.selected = change.key;
      this.selectedCue = this.subtitles.selected;
    }
  }
  async navigate(forward: boolean) {
    if (!this.workspace.canAct || !this.page?.statistics) return;
    const key = this.key();
    try {
      const reply = await reviewAction({
        kind: 'navigate',
        key,
        from:
          this.subtitles.selected === this.selectedCue ? this.selected : null,
        cueId: this.subtitles.selected,
        forward,
        expanded: this.expanded,
        filter: this.filter,
      });
      if (
        this.alive &&
        reply.rows[0] &&
        JSON.stringify(key) === JSON.stringify(this.key())
      ) {
        if (reply.position)
          this.offset = Math.floor((reply.position - 1) / 100) * 100;
        const block = reply.blocks?.[0];
        await this.select(
          reply.rows[0],
          block && !this.expanded.includes(block.block.first)
            ? block
            : undefined,
        );
      }
    } catch (error) {
      if (this.alive) this.error = String(error);
    }
  }
  async revert(change: ReviewChange | null) {
    if (!this.canMutate) return;
    await this.workspace.task(async () => {
      await this.subtitles.changeReview(
        {
          kind: 'revert',
          key: this.key(),
          changeKey: change?.key ?? null,
          sourceIndex:
            change?.original[0]?.source_index ??
            change?.revised[0]?.source_index ??
            null,
          filter: change ? null : this.filter,
        },
        this.subtitles.snapshot,
      );
    });
  }
  async revertBlock(first: number) {
    if (
      !this.canMutate ||
      JSON.stringify(this.page?.key) !== JSON.stringify(this.key())
    )
      return;
    await this.workspace.task(() =>
      this.subtitles.changeReview(
        { kind: 'revertBlock', key: this.key(), first },
        this.subtitles.snapshot,
      ),
    );
  }
  toggleBlock(first: number) {
    this.expanded = this.expanded.includes(first)
      ? this.expanded.filter((i) => i !== first)
      : [...this.expanded, first];
  }
  async setOptions(options: NonNullable<ReviewReply['context']>['options']) {
    if (!this.workspace.canAct) return;
    await this.workspace.task(() =>
      this.subtitles.changeReview(
        { kind: 'options', key: this.key(), options },
        this.subtitles.snapshot,
      ),
    );
  }
  decideExchange: (prompt: ExchangePrompt) => Promise<ExchangeAnswer> =
    async () => 'cancel';
  async exportReport(kind: 'report' | 'package' = 'report') {
    if (!this.workspace.canAct || !this.page?.statistics) return;
    await this.workspace.task(async () => {
      if (this.subtitles.dirty)
        throw new Error('Finish the current cue edit before exporting it.');
      const expected = this.subtitles.snapshot;
      const key = this.key();
      const answer = await this.decideExchange({
        kind,
        dirty: expected.dirty,
        author: this.name(),
      });
      if (
        !this.alive ||
        typeof answer === 'string' ||
        expected !== this.subtitles.snapshot
      )
        return;
      const suffix = kind === 'report' ? '.comparison.html' : '.vsfreview';
      const stem =
        expected.path ?? expected.suggestedSavePath ?? 'Untitled.srt';
      const path = await chooseFile(
        kind === 'report' ? 'comparisonReport' : 'saveReviewPackage',
        stem.replace(/\.[^./\\]+$/u, '') + suffix,
      );
      if (!path || !this.alive) return;
      await this.subtitles.changeReview(
        {
          kind: 'export',
          key,
          path,
          output: answer.kind === 'readable' ? 'readable' : kind,
          author: answer.author,
          date: new Intl.DateTimeFormat(undefined, {
            dateStyle: 'medium',
            timeStyle: 'short',
          }).format(),
          includeUnchanged: this.includeUnchanged,
        },
        expected,
      );
      if (this.alive)
        this.workspace.notice =
          kind === 'report'
            ? 'Comparison report exported'
            : 'Review package exported';
    });
  }
  async openPackage() {
    if (!this.workspace.canAct) return;
    await this.workspace.task(async () => {
      const path = await chooseFile(
        'reviewPackage',
        this.subtitles.snapshot.path,
      );
      if (!path || !this.alive) return;

      let key = this.key();
      const inspected = await reviewAction({
        kind: 'inspectPackage',
        key,
        path,
      });
      const info = inspected.inspection;
      if (
        !info ||
        !this.alive ||
        JSON.stringify(key) !== JSON.stringify(this.key())
      )
        return;
      let choice: 'open' | 'merge' | 'keep' | 'useTheirs';
      if (info.case === 'new') {
        if (!(await this.unsaved.protect(this.saveProject)) || !this.alive)
          return;
        const protectedKey = this.key();
        if (protectedKey.generation !== key.generation) return;
        if (JSON.stringify(key) !== JSON.stringify(protectedKey)) {
          const checked = await reviewAction({
            kind: 'inspectPackage',
            key: protectedKey,
            path,
          });
          if (
            checked.inspection?.case !== 'new' ||
            checked.inspection.digest !== info.digest
          )
            throw new Error('The review changed. Open the package again.');
          key = protectedKey;
        }
        choice = 'open';
      } else if (info.case === 'different') {
        const answer = await this.decideExchange({
          kind: 'different',
          author: info.exportedBy,
          time: info.exportedAtMs,
        });
        if (!this.alive || (answer !== 'theirs' && answer !== 'mine')) return;
        choice = answer === 'theirs' ? 'useTheirs' : 'keep';
      } else choice = 'merge';
      const adopted = await this.subtitles.changeReview(
        {
          kind: 'importPackage',
          key,
          path,
          digest: info.digest,
          choice,
          discard: choice === 'open',
        },
        this.subtitles.snapshot,
        (notice) => {
          if (this.alive) this.workspace.notice = notice;
        },
      );
      if (adopted && this.alive) {
        if (choice === 'open') await this.detachProject();
        this.offset = 0;
        this.view.openReview('changes');
      }
    });
  }
  key() {
    const s = this.subtitles.snapshot;
    return {
      generation: s.generation,
      revision: s.revision,
      epoch: s.review?.epoch ?? 0,
    };
  }
  private async query(
    capture: string,
    offset: number,
    filter: typeof this.filter,
    sequence: number,
    expanded: number[],
    threadFilter: typeof this.threadFilter,
  ) {
    try {
      let reply = await reviewAction({
        kind: 'query',
        key: this.key(),
        offset,
        limit: 100,
        expanded,
        filter,
      });
      if (threadFilter && reply.statistics && !reply.working)
        reply = await reviewAction({
          kind: 'threads',
          key: reply.key,
          open: threadFilter === 'open',
          offset,
          limit: 100,
        });
      const key = this.key();
      if (
        !this.alive ||
        sequence !== this.sequence ||
        capture !== `${key.generation}/${key.revision}/${key.epoch}`
      )
        return;
      this.page = reply;
      this.error = reply.error ?? '';
      if (reply.working)
        setTimeout(() => {
          if (this.alive && sequence === this.sequence)
            void this.query(
              capture,
              offset,
              filter,
              sequence,
              expanded,
              threadFilter,
            );
        }, 75);
    } catch (error) {
      if (this.alive && sequence === this.sequence) this.error = String(error);
    }
  }
  run = async (
    operation: 'startReview' | 'compareOriginal' | 'compareFiles' | 'endReview',
  ) => {
    if (!this.workspace.canAct) return;
    await this.workspace.task(async () => {
      if (operation === 'endReview') {
        await this.subtitles.changeReview(
          { kind: 'detach', key: this.key() },
          this.subtitles.snapshot,
        );
        return;
      }
      if (operation === 'startReview') {
        if (!(await this.unsaved.protect(this.saveProject)) || !this.alive)
          return;
        const original = this.subtitles.snapshot.path;
        if (!original)
          throw new Error('Save the subtitles before starting a review');
        // Discard reloads accepted saved content; the Rust copy command preserves attached contexts.
        const proposal = original.replace(/(\.[^./\\]+)$/u, '.reviewed$1');
        const path = await chooseFile('saveSubtitles', proposal);
        if (!path || !this.alive) return;
        await this.subtitles.changeReview(
          { kind: 'start', key: this.key(), path, discard: true },
          this.subtitles.snapshot,
        );
      } else {
        let original = await chooseFile(
          'reviewOriginal',
          this.subtitles.snapshot.review?.path ?? this.subtitles.snapshot.path,
        );
        if (!original || !this.alive) return;
        let revised =
          operation === 'compareFiles'
            ? await chooseFile('reviewRevised', this.subtitles.snapshot.path)
            : this.subtitles.snapshot.path;
        if (!this.alive || (operation === 'compareFiles' && !revised)) return;
        const inspect = revised
          ? await reviewAction({
              kind: 'inspect',
              original,
              revised,
            })
          : null;
        if (!this.alive) return;
        let replace = operation === 'compareFiles';
        if (inspect?.newerOriginal && revised) {
          const choice = await this.decideSwap(original);
          if (!this.alive || choice === 'cancel') return;
          if (choice === 'swap') {
            [original, revised] = [revised, original];
            replace = true;
          }
        }
        if (replace && revised) {
          if (!(await this.unsaved.protect(this.saveProject)) || !this.alive)
            return;
          if (
            !(await this.subtitles.command({
              kind: 'open',
              path: revised,
              discard: true,
            }))
          )
            return;
        }
        await this.subtitles.changeReview(
          { kind: 'attach', key: this.key(), path: original },
          this.subtitles.snapshot,
        );
      }
      if (this.alive) {
        this.view.openReview('changes');
        if (!this.subtitles.snapshot.path)
          this.workspace.notice =
            'Save the subtitles before saving a review project.';
      }
    });
  };
  dispose() {
    this.alive = false;
    ++this.sequence;
  }
}
