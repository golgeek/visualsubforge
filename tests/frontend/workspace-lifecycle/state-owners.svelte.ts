import { WorkspacePreferences } from '../../../src/lib/workspace-preferences.svelte';
import { WorkspaceView } from '../../../src/lib/workspace-view.svelte';
import { WorkspaceDecision } from '../../../src/lib/workspace-decision.svelte';
import { defaultPreferences } from '../../../src/lib/client';

export async function stateOwnerCases(passed: (message: string) => void) {
  const assert = (condition: unknown, message: string) => {
    if (!condition) throw new Error(message);
  };
  const preferences = new WorkspacePreferences();
  const saved = {
    ...defaultPreferences,
    showQualityMetrics: true,
    timing: { ...defaultPreferences.timing, cueGapMs: 123 },
  };
  preferences.initialize(saved);
  preferences.restoreTiming({ ...saved.timing, cueGapMs: 456 });
  assert(
    preferences.timingSettings.cueGapMs === 456 &&
      preferences.preferences.timing.cueGapMs === 123,
    'Project timing must not replace saved defaults',
  );
  assert(
    preferences.showQualityMetrics && preferences.qualityContext === 1,
    'Restoration invalidates quality without changing metrics visibility',
  );
  preferences.resetTiming();
  assert(
    preferences.timingSettings.cueGapMs === 123 &&
      preferences.qualityContext === 1,
    'Media replacement restores saved timing without a quality-context bump',
  );
  preferences.accept({ ...saved, showQualityMetrics: false });
  assert(
    !preferences.showQualityMetrics && preferences.qualityContext === 2,
    'Settings acceptance updates visibility and invalidates quality once',
  );
  saved.timing.cueGapMs = 999;
  assert(
    preferences.preferences.timing.cueGapMs === 123,
    'Saved timing must be copied from caller-owned input',
  );
  passed(
    'Preferences preserve saved defaults across project/media timing and settings acceptance',
  );

  const panels = new WorkspaceView();
  let cancelled = 0;
  panels.translationOpen = true;
  panels.toggleProblems(() => ++cancelled);
  assert(
    panels.problemsOpen && !panels.translationOpen && cancelled === 0,
    'Opening Problems hides Translation without cancelling navigation',
  );
  panels.toggleTranslation();
  assert(
    panels.translationOpen && !panels.problemsOpen && cancelled === 0,
    'Translation toggle preserves the existing pending-navigation policy',
  );
  panels.revealProblems();
  assert(
    panels.problemsOpen && panels.translationOpen,
    'Statistics reveal preserves translation visibility',
  );
  panels.closeProblems(() => ++cancelled);
  assert(
    !panels.problemsOpen && cancelled === 1,
    'Explicit Problems close cancels its pending navigation',
  );
  passed('Panel owner preserves toggle, close and Statistics reveal policies');

  let finishHide!: () => void;
  let finishRestore!: () => void;
  const decision = new WorkspaceDecision<'yes' | 'cancel', string>(
    'cancel',
    { error: '' },
    {
      occlude: (hidden) =>
        new Promise<void>((resolve) => {
          if (hidden) finishHide = resolve;
          else finishRestore = resolve;
        }),
      afterRender: async () => {},
      remainingOcclusion: () => false,
      close: () => {},
    },
  );
  const opening = decision.show('captured issue');
  assert(
    decision.payload === null,
    'Payload must not show before native video hides',
  );
  finishHide();
  await opening;
  assert(
    decision.payload === 'captured issue',
    'Decision owns the presented issue',
  );
  let settled = false;
  const answer = decision.wait().then((value) => {
    settled = true;
    return value;
  });
  const closing = decision.answer('yes');
  await Promise.resolve();
  await Promise.resolve();
  assert(
    decision.payload === null && !settled,
    'Hide the payload before settling its answer',
  );
  finishRestore();
  await closing;
  assert(
    (await answer) === 'yes',
    'Settle only after remaining occlusion is restored',
  );
  decision.dispose();
  passed(
    'Decision payload shares hide-before-show and restore-before-answer ordering',
  );
}
