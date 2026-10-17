import {
  chooseFile,
  openProject,
  attachProject,
  detachProject,
  saveProject,
  restoreScenes,
  finishExit,
  type ProjectAttachment,
  type ProjectOptions,
} from './client';
import type { DocumentSession } from './document-session.svelte';
import type { MediaSession } from './media-session.svelte';
import type { ProjectExitChoice } from './ProjectSaveFailureDialog.svelte';

const client = {
  chooseFile,
  openProject,
  attachProject,
  detachProject,
  saveProject,
  restoreScenes,
  finishExit,
};
export interface ProjectWorkspace {
  read(): { mediaPath: string; hardware: boolean; options: ProjectOptions };
  beginRestore(): void;
  mediaOpened(mediaPath: string): void;
  restoreOptions(options: ProjectOptions): void;
  restoreScenes(session: number, generation: number): void;
  protect(): Promise<boolean>;
  saveSubtitles(): Promise<boolean>;
  saveFailed(error: unknown): Promise<ProjectExitChoice>;
  notify(message: string): void;
}

// Rust owns the attachment token/baseline. This projection is adopted only after
// a complete restore/save. A failed open cannot autosave the old project path.
export class ProjectWorkflow {
  private attachment = $state.raw<ProjectAttachment | null>(null);
  private alive = true;
  private operation = 0;
  private document: DocumentSession;
  private media: MediaSession;
  private workspace: ProjectWorkspace;
  private client: typeof client;

  constructor(
    document: DocumentSession,
    media: MediaSession,
    workspace: ProjectWorkspace,
    api = client,
  ) {
    this.document = document;
    this.media = media;
    this.workspace = workspace;
    this.client = api;
  }
  get path() {
    return this.attachment?.path ?? null;
  }
  private check(operation: number, session?: number) {
    if (!this.alive || operation !== this.operation)
      throw new Error('Project operation cancelled');
    if (session !== undefined && this.media.status.session !== session)
      throw new Error('Media changed during project operation');
  }
  async detach() {
    ++this.operation;
    const attached = this.attachment !== null;
    this.attachment = null;
    if (this.alive && attached) await this.client.detachProject();
  }
  async open() {
    const operation = ++this.operation;
    this.check(operation);
    const path = await this.client.chooseFile('project', this.path);
    this.check(operation);
    if (!path) return;
    // Validate references before the user discards or saves the active draft.
    const project = await this.client.openProject(path);
    this.check(operation);
    const proceed = await this.workspace.protect();
    this.check(operation);
    if (!proceed) return;
    this.attachment = null;
    this.workspace.beginRestore();
    if (
      !(await this.media.command({
        kind: 'open',
        path: project.mediaPath,
        hardware: this.workspace.read().hardware,
      }))
    )
      return;
    this.check(operation);
    this.workspace.mediaOpened(project.mediaPath);
    const session = this.media.status.session;
    await this.media.waitUntilLoaded(session);
    this.check(operation, session);
    if (project.subtitlePath) {
      if (
        !(await this.document.command({
          kind: 'open',
          path: project.subtitlePath,
          discard: true,
        }))
      )
        return;
    } else if (this.document.snapshot.loaded) {
      if (!(await this.document.command({ kind: 'close', discard: true })))
        return;
    }
    this.check(operation, session);
    if (project.scenes?.mode === 'visual') {
      const generation = await this.client.restoreScenes(
        project.mediaPath,
        session,
        project.scenes,
      );
      this.check(operation, session);
      this.workspace.restoreScenes(session, generation);
    }
    if (
      !(await this.media.command({
        kind: 'seek',
        seconds: Math.min(
          project.positionSeconds,
          this.media.status.durationSeconds ?? project.positionSeconds,
        ),
      }))
    )
      return;
    this.check(operation, session);
    const attached = await this.client.attachProject(
      project.token,
      session,
      this.document.snapshot,
    );
    this.check(operation, session);
    this.attachment = attached;
    this.workspace.restoreOptions(project.options);
    this.workspace.notify(
      project.scenes?.mode === 'keyframes'
        ? 'Project restored · playback paused · keyframe markers skipped; run a visual scan'
        : 'Project restored · playback paused',
    );
  }
  async save(as: boolean) {
    return this.saveAt(++this.operation, as, true);
  }
  private async saveAt(
    operation: number,
    as: boolean,
    saveSubtitles: boolean,
  ): Promise<boolean> {
    this.check(operation);
    const session = this.media.status.session;
    if (!this.media.status.loaded || !this.workspace.read().mediaPath)
      return false;
    const doc = this.document.snapshot;
    if (
      saveSubtitles &&
      doc.loaded &&
      (doc.dirty || this.document.dirty || doc.requiresSaveAs)
    ) {
      const saved = await this.workspace.saveSubtitles();
      this.check(operation, session);
      if (!saved) return false;
    }
    const path =
      as || !this.path
        ? await this.client.chooseFile(
            'saveProject',
            this.path ??
              this.workspace.read().mediaPath.replace(/\.[^/.\\]+$/, '') +
                '.vsfproject',
          )
        : this.path;
    this.check(operation, session);
    if (!path) return false;
    const current = this.workspace.read();
    const attached = await this.client.saveProject({
      path,
      token: this.attachment?.token ?? null,
      mediaPath: current.mediaPath,
      subtitlePath: this.document.snapshot.path,
      session,
      positionSeconds: this.media.status.positionSeconds ?? 0,
      options: current.options,
    });
    this.check(operation, session);
    this.attachment = attached;
    this.workspace.notify('Project saved');
    return true;
  }
  async quit() {
    const operation = ++this.operation;
    this.check(operation);
    const proceed = await this.workspace.protect();
    this.check(operation);
    if (!proceed) return;
    if (this.path && this.media.status.loaded) {
      let saveAs = false;
      while (true) {
        this.check(operation);
        try {
          if (!(await this.saveAt(operation, saveAs, false))) return;
          break;
        } catch (error) {
          this.check(operation);
          const choice = await this.workspace.saveFailed(error);
          this.check(operation);
          if (choice === 'cancel') return;
          if (choice === 'quit') break;
          if (choice === 'saveAs') saveAs = true;
        }
      }
    }
    this.check(operation);
    await this.client.finishExit(this.document.snapshot, true);
  }
  dispose() {
    this.alive = false;
    ++this.operation;
  }
}
