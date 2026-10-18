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
  type ProjectReferenceIssue,
  type ProjectReferenceOverride,
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
  protect(saveContext?: () => Promise<boolean>): Promise<boolean>;
  referenceIssue?(
    issue: ProjectReferenceIssue,
  ): Promise<'cancel' | 'choose' | 'reload'>;
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
    return this.openAt(operation, path);
  }
  async openPath(path: string) {
    return this.openAt(++this.operation, path);
  }
  private async openAt(operation: number, path: string) {
    this.check(operation);
    // Validate references before the user discards or saves the active draft.
    let reference: ProjectReferenceOverride | null = null;
    const prepare = async () => {
      while (true) {
        const project = await this.client.openProject(path, reference);
        this.check(operation);
        if (!project.referenceIssue) {
          if (project.mediaPath && !this.media.status.available)
            throw new Error(
              'This project includes media and requires a build with native playback enabled',
            );
          return project;
        }
        if (!this.workspace.referenceIssue)
          throw new Error(project.referenceIssue.message);
        const choice = await this.workspace.referenceIssue(
          project.referenceIssue,
        );
        this.check(operation);
        if (choice === 'cancel') return null;
        if (choice === 'reload' && project.referenceIssue.canReload)
          reference = { path: reference?.path ?? null, reload: true };
        else {
          const selected = await this.client.chooseFile(
            'subtitles',
            project.referenceIssue.path,
          );
          this.check(operation);
          if (selected) reference = { path: selected, reload: false };
        }
      }
    };
    let project = await prepare();
    if (!project) return;
    let restage = false;
    const proceed = await this.workspace.protect(async () => {
      const saved = await this.saveAt(operation, false, true);
      restage = saved;
      return saved;
    });
    this.check(operation);
    if (!proceed) return;
    // Saving the current project invalidates staged tokens. Revalidate the next
    // project's references before replacing any current workspace state.
    if (restage) {
      project = await prepare();
      if (!project) return;
    }
    this.attachment = null;
    this.workspace.beginRestore();
    let session = this.media.status.session;
    if (project.mediaPath) {
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
      session = this.media.status.session;
      await this.media.waitUntilLoaded(session);
    } else if (this.media.status.available) {
      if (!(await this.media.command({ kind: 'close' }))) return;
      session = this.media.status.session;
    }
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
    if (project.mediaPath && project.scenes?.mode === 'visual') {
      const generation = await this.client.restoreScenes(
        project.mediaPath,
        session,
        project.scenes,
      );
      this.check(operation, session);
      this.workspace.restoreScenes(session, generation);
    }
    if (
      project.mediaPath &&
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
    if (!(await this.document.command({ kind: 'status' }))) return;
    this.check(operation, session);
    this.attachment = attached;
    this.workspace.restoreOptions(project.options);
    this.workspace.notify(
      !project.mediaPath
        ? 'Subtitle project restored'
        : project.scenes?.mode === 'keyframes'
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
    const doc = this.document.snapshot;
    if (!this.media.status.loaded && !doc.loaded) return false;
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
              (this.media.status.loaded
                ? this.workspace.read().mediaPath
                : (this.document.snapshot.path ?? 'subtitles')
              ).replace(/\.[^/.\\]+$/, '') + '.vsfproject',
          )
        : this.path;
    this.check(operation, session);
    if (!path) return false;
    const current = this.workspace.read();
    const attached = await this.client.saveProject({
      path,
      token: this.attachment?.token ?? null,
      mediaPath: this.media.status.loaded ? current.mediaPath : null,
      subtitlePath: this.document.snapshot.path,
      session,
      positionSeconds: this.media.status.loaded
        ? (this.media.status.positionSeconds ?? 0)
        : 0,
      options: current.options,
    });
    this.check(operation, session);
    this.attachment = attached;
    if (!(await this.document.command({ kind: 'status' }))) return false;
    this.check(operation, session);
    this.workspace.notify('Project saved');
    return true;
  }
  async quit() {
    const operation = ++this.operation;
    this.check(operation);
    const contextWasDirty = this.document.snapshot.translation?.dirty === true;
    const proceed = await this.workspace.protect(() =>
      this.saveAt(operation, false, true),
    );
    this.check(operation);
    if (!proceed) return;
    // Context Save already wrote the project; Discard must leave its previous
    // reference relationship intact rather than autosaving the discarded choice.
    if (
      !contextWasDirty &&
      this.path &&
      (this.media.status.loaded || this.document.snapshot.loaded)
    ) {
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
