import {
  defaultPreferences,
  type Preferences,
  type ProjectReferenceIssue,
} from './client';
import { defaultTiming } from './magnetic-timing';
/** Transient workspace view preferences and panel state, separate from documents. */
export class WorkspaceView {
  translationOpen = $state(false);
  problemsOpen = $state(false);
  showQualityMetrics = $state(false);
  qualityContext = $state(0);
  follow = $state(true);
  waveformGesture = $state(false);
  mediaPath = $state('');
  restoredScenes = $state<{ session: number; generation: number } | null>(null);
  showScenes = $state(true);
  preferences = $state<Preferences>({
    ...defaultPreferences,
    timing: { ...defaultTiming },
  });
  legacyImportOpen = $state(false);
  legacyImportReturn: HTMLElement | null = null;
  hardware = $state(true);
  diagnostics = $state(false);
  timingSettings = $state({ ...defaultTiming });
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
  projectExitError = $state<string | null>(null);
  projectReferenceIssue = $state<ProjectReferenceIssue | null>(null);
}
