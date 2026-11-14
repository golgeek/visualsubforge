import { chooseFile, listenFileDrop } from '../client';
import type { Workspace } from '../workspace/workspace.svelte';
import { classifyFiles, type OpenFile } from './file-kinds';
const client = { chooseFile, listenFileDrop };
/** Admission and native subscription only; existing feature owners perform opens. */
export class WorkspaceOpen {
  private alive = true;
  private unlisten: (() => void) | null = null;
  constructor(
    private workspace: Workspace,
    private open: (file: OpenFile) => Promise<boolean>,
    private prepare: () => Promise<void>,
    private api = client,
  ) {}
  async startListening() {
    try {
      const stop = await this.api.listenFileDrop((paths) => {
        void this.paths(paths);
      });
      if (this.alive) this.unlisten = stop;
      else stop();
    } catch (error) {
      if (this.alive) this.workspace.error = String(error);
    }
  }
  async pick() {
    if (!this.alive) return;
    await this.prepare();
    if (!this.alive || !this.workspace.canAct) return;
    await this.workspace.task(async () => {
      const path = await this.api.chooseFile('workspace', null);
      if (this.alive && path) await this.openFiles(classifyFiles([path]));
    });
  }
  async paths(paths: string[]) {
    if (!this.alive) return;
    await this.prepare();
    if (!this.alive || !this.workspace.canAct) return;
    await this.workspace.task(async () => {
      await this.openFiles(classifyFiles(paths));
    });
  }
  private async openFiles(files: OpenFile[]) {
    for (const file of files) {
      if (!this.alive || !(await this.open(file))) return;
    }
  }
  dispose() {
    this.alive = false;
    this.unlisten?.();
    this.unlisten = null;
  }
}
