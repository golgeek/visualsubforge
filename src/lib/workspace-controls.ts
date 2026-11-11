import type { WorkspaceAction } from './client';

// App implements this boundary; components never own or assign workspace gates.
export interface WorkspaceControls {
  readonly busy: boolean;
  readonly confirming: boolean;
  readonly ready: boolean;
  run: (action: WorkspaceAction, event?: Event) => Promise<void>;
  notice: (message: string) => void;
}
