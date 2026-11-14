/** Scene controls and the current analysis projection; Rust owns accepted cuts. */
export class WorkspaceScenes {
  showScenes = $state(true);
  sceneCuts = $state<number[]>([]);
  sceneControls = $state<{
    rescan: () => Promise<void>;
    cancel: () => Promise<void>;
    navigate: (backwards: boolean) => void;
  }>();
  sceneState = $state({
    qualityKey: '',
    running: false,
    pending: false,
    previous: false,
    next: false,
  });
  restoredScenes = $state<{ session: number; generation: number } | null>(null);
  restore(session: number, generation: number) {
    this.restoredScenes = { session, generation };
  }
  clearRestored() {
    this.restoredScenes = null;
  }
}
