import { languageName } from './language';
import { chooseFile, extractionAction, type ExtractionReply } from '../client';
import type { DocumentSession } from '../document/document-session.svelte';
import type { WorkspaceModal } from '../dialogs/workspace-modal.svelte';
import type { Workspace } from '../workspace/workspace.svelte';

/** Transient chooser/job lifetime. Rust alone owns prepared and accepted content. */
export class ExtractionSession {
  path = $state('');
  view = $state.raw<ExtractionReply | null>(null);
  selected = $state<number | null>(null);
  busy = $state(false);
  extracting = $state<number | null>(null);
  result = $state.raw<{ generation: number; message: string } | null>(null);
  error = $state('');
  elapsed = $state(0);
  notice = $state.raw<{ path: string; count: number } | null>(null);
  // Session bookkeeping must not retrigger the media observer when marked seen.
  // eslint-disable-next-line svelte/prefer-svelte-reactivity
  private seen = new Set<string>();
  private epoch = 0;
  private alive = true;
  private id: number | null = null;
  constructor(
    private workspace: Workspace,
    private subtitles: DocumentSession,
    private modal: WorkspaceModal,
    private protect: () => Promise<boolean>,
    private openMedia: (path: string) => Promise<unknown>,
  ) {}
  async open(path?: string) {
    const chosen = path ?? (await chooseFile('extractSubtitles', null));
    if (!chosen || !this.alive) return;
    this.path = chosen;
    this.notice = null;
    this.seen.add(chosen);
    await this.modal.open('extraction');
    if (this.modal.identity !== 'extraction' || !this.alive) return;
    await this.load(chosen);
  }
  private async job(
    action: Parameters<typeof extractionAction>[0],
    epoch: number,
  ) {
    let reply = await extractionAction(action);
    if (!this.alive || epoch !== this.epoch) {
      await extractionAction({ kind: 'cancel', id: reply.id });
      return null;
    }
    this.id = reply.id;
    const started = Date.now();
    while (reply.status === 'running') {
      await new Promise((resolve) => setTimeout(resolve, 100));
      if (!this.alive || epoch !== this.epoch) return null;
      this.elapsed = Math.floor((Date.now() - started) / 1000);
      reply = await extractionAction({ kind: 'view', id: reply.id });
    }
    if (!this.alive || epoch !== this.epoch) return null;
    if (reply.error) throw new Error(reply.error);
    return reply.status === 'cancelled' ? null : reply;
  }
  private async load(path: string) {
    const epoch = ++this.epoch;
    this.busy = true;
    this.error = '';
    this.view = null;
    this.elapsed = 0;
    this.extracting = null;
    try {
      const reply = await this.job({ kind: 'list', path }, epoch);
      if (!reply) return;
      this.view = reply;
      const usable = reply.listing?.tracks.filter((t) => !t.unavailable) ?? [];
      this.selected =
        (usable.find((t) => t.default) ?? usable[0])?.index ?? null;
    } catch (error) {
      if (this.alive && epoch === this.epoch) {
        this.error = String(error);
        if (this.modal.identity !== 'extraction')
          this.workspace.error = this.error;
      }
    } finally {
      if (this.alive && epoch === this.epoch) this.busy = false;
    }
  }
  async extract() {
    if (this.busy || this.selected === null) return;
    const path = this.path,
      index = this.selected;
    // Close chooser before the shared HideVideo decision is presented.
    await this.modal.close();
    if (!this.alive || !(await this.protect()) || !this.alive) return;
    const expected = this.subtitles.snapshot;
    await this.modal.open('extraction');
    if (this.modal.identity !== 'extraction') return;
    const epoch = ++this.epoch;
    this.busy = true;
    this.error = '';
    this.elapsed = 0;
    this.extracting = index;
    try {
      const reply = await this.job(
        {
          kind: 'start',
          path,
          index,
          generation: expected.generation,
          revision: expected.revision,
          discard: true,
        },
        epoch,
      );
      if (!reply || !this.alive || epoch !== this.epoch) return;
      if (!(await this.subtitles.acceptExtraction(reply.id, expected))) return;
      this.result = {
        generation: this.subtitles.snapshot.generation,
        message:
          `Extracted ${reply.cueCount} cue${reply.cueCount === 1 ? '' : 's'} from track ${index} (${languageName(reply.provenance?.language)}).` +
          (reply.provenance?.codec === 'mov_text'
            ? ' Converted from MP4 text; styling may differ.'
            : '') +
          (reply.fontCount
            ? ` This video has ${reply.fontCount} font attachment${reply.fontCount === 1 ? '' : 's'}.`
            : '') +
          (reply.repairedCues.length
            ? ` Repaired missing durations for cues ${reply.repairedCues.join(', ')}.`
            : ''),
      };
      await this.modal.close();
      if (this.alive) await this.openMedia(path);
    } catch (error) {
      if (this.alive && epoch === this.epoch) {
        this.error = String(error);
        if (this.modal.identity !== 'extraction')
          this.workspace.error = this.error;
      }
    } finally {
      if (this.alive && epoch === this.epoch) this.busy = false;
    }
  }
  async cancel() {
    this.extracting = null;
    ++this.epoch;
    const id = this.id;
    this.id = null;
    this.busy = false;
    if (id !== null)
      await extractionAction({ kind: 'cancel', id }).catch(() => {});
    await this.modal.close();
  }
  async observe(path: string, loaded: boolean) {
    this.notice = null;
    if (!this.busy && !this.modal.active) ++this.epoch;
    if (
      !path ||
      loaded ||
      this.busy ||
      this.modal.active ||
      this.seen.has(path)
    )
      return;
    this.seen.add(path);
    const epoch = ++this.epoch;
    try {
      const reply = await this.job({ kind: 'list', path }, epoch);
      if (!reply || this.subtitles.snapshot.loaded) return;
      const count =
        reply.listing?.tracks.filter((t) => !t.unavailable).length ?? 0;
      if (count) this.notice = { path, count };
    } catch {
      /* Optional notice must never interrupt media opening. */
    }
  }
  dismiss() {
    this.notice = null;
  }
  dispose() {
    this.alive = false;
    ++this.epoch;
    if (this.id !== null)
      void extractionAction({ kind: 'cancel', id: this.id }).catch(() => {});
  }
}
