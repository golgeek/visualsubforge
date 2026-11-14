import { WorkspaceLayout } from '../../../src/lib/layout/workspace-layout.svelte';
import { WorkspacePreferences } from '../../../src/lib/settings/workspace-preferences.svelte';
import { defaultLayout } from '../../../src/lib/layout/layout-presets';
import { presetArrangement } from '../../../src/lib/layout/arrangement';
import type { Preferences } from '../../../src/lib/client';

export async function ownerChecks(passed: (message: string) => void) {
  const assert = (value: unknown, message: string) => {
    if (!value) throw new Error(message);
  };
  const pause = () => new Promise((resolve) => setTimeout(resolve, 300));
  const writes: Preferences['layout'][] = [];
  const errors: unknown[] = [];
  let release: (() => void) | null = null;
  let hold = false;
  const owner = new WorkspaceLayout(
    new WorkspacePreferences(),
    { cue: undefined },
    {
      occlude: async (hidden) => {
        if (hidden && hold)
          await new Promise<void>((resolve) => (release = resolve));
      },
      afterRender: async () => {},
      remainingOcclusion: () => false,
      preset: () => {},
      error: (error) => errors.push(error),
    },
    async (value) => {
      writes.push(value);
    },
  );
  try {
    await owner.select('review');
    await pause();
    assert(writes.length === 1, 'User layout persisted');
    await owner.rememberForProject(true);
    await owner.select('bigPicture');
    await pause();
    assert(
      writes.length === 1 && owner.projectSnapshot?.active === 'bigPicture',
      'Project changes never overwrite user defaults',
    );
    await owner.restoreProject(null);
    assert(
      owner.preset === 'review' && owner.projectSnapshot === null,
      'Old project restores user layout instead of previous project override',
    );
    passed(
      'project layouts and old-project restore preserve independent user defaults',
    );
    await owner.rememberForProject(true);
    await owner.select('editorAbove');
    await owner.rememberForProject(false);
    assert(
      owner.preset === 'review' && owner.projectSnapshot === null,
      'Removing override returns to user layout',
    );
    passed(
      'removing a project override produces an explicit null save snapshot',
    );
    const project = defaultLayout();
    project.active = 'translation';
    hold = true;
    let current = true;
    const restoring = owner
      .restoreProject(project, () => {
        if (!current) throw new Error('Obsolete restore');
      })
      .catch((error) => error);
    while (!release) await new Promise((resolve) => setTimeout(resolve, 0));
    assert(owner.preset === 'review', 'No layout mutation before hide');
    current = false;
    hold = false;
    (release as () => void)();
    release = null;
    const failure = await restoring;
    assert(
      failure instanceof Error &&
        owner.preset === 'review' &&
        errors.length === 1,
      'Queued obsolete restore cannot publish layout',
    );
    passed(
      'project operation guard retires layout restoration waiting on native hiding',
    );
    owner.workspaceWidth = 1100;
    owner.workspaceHeight = 660;
    owner.viewportHeight = 720;
    await owner.arrange(presetArrangement('timing', 65, false, false));
    await owner.synchronizeDocks(true, false);
    assert(
      owner.custom?.rows.some((row) => row.panels.includes('review')),
      'Tool reveals its hidden slot',
    );
    await owner.synchronizeDocks(false, false);
    assert(
      !owner.custom?.rows.some((row) => row.panels.includes('review')),
      'Closed tool removes its slot',
    );
    passed(
      'tool visibility reconciles custom slots without invisible review owners',
    );
  } finally {
    owner.dispose();
  }
}
