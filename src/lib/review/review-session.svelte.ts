import { chooseFile, reviewAction, type ReviewReply } from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
import type { UnsavedChanges } from '../document/unsaved-changes';
import type { Workspace } from '../workspace/workspace.svelte';
import type { WorkspaceView } from '../workspace/workspace-view.svelte';
export class ReviewSession {
  page = $state.raw<ReviewReply | null>(null);
  offset = $state(0);
  filter = $state<ReviewReply['rows'][number]['kind'] | null>(null);
  selected = $state<string | null>(null);
  error = $state('');
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
      const offset = this.offset;
      const filter = this.filter;
      const sequence = ++this.sequence;
      this.page = null;
      if (!snapshot.review?.path) return;
      const timer = setTimeout(
        () => void this.query(capture, offset, filter, sequence),
        250,
      );
      return () => {
        clearTimeout(timer);
      };
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
  ) {
    try {
      const reply = await reviewAction({
        kind: 'query',
        key: this.key(),
        offset,
        limit: 100,
        filter,
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
            void this.query(capture, offset, filter, sequence);
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
        if (!revised || !this.alive) {
          if (operation === 'compareOriginal')
            throw new Error('Save the working document first');
          return;
        }
        const inspect = await reviewAction({
          kind: 'inspect',
          original,
          revised,
        });
        if (!this.alive) return;
        let replace = operation === 'compareFiles';
        if (inspect.newerOriginal) {
          const choice = await this.decideSwap(original);
          if (!this.alive || choice === 'cancel') return;
          if (choice === 'swap') {
            [original, revised] = [revised, original];
            replace = true;
          }
        }
        if (replace) {
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
      if (this.alive) this.view.openReview('changes');
    });
  };
  dispose() {
    this.alive = false;
    ++this.sequence;
  }
}
