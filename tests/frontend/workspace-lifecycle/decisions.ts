import type { LegacyImportChoice } from '../../../src/lib/LegacyImportDialog.svelte';
import { WorkspaceDecision } from '../../../src/lib/workspace-decision.svelte';
import type { ProjectExitChoice } from '../../../src/lib/ProjectSaveFailureDialog.svelte';

export async function decisionCases(passed: (message: string) => void) {
  const assert = (value: unknown, message: string) => {
    if (!value) throw new Error(message);
  };
  async function check<Choice>(
    name: string,
    cancelled: Choice,
    accepted: Choice,
  ) {
    const events: string[] = [];
    let failHide = false;
    let failRestore = false;
    let releaseHide: (() => void) | null = null;
    let hide: Promise<void> | null = null;
    const create = () =>
      new WorkspaceDecision(
        cancelled,
        {
          set error(value: string) {
            events.push(value);
          },
        },
        {
          occlude: async (value) => {
            events.push(value ? 'hide' : 'restore');
            if (value) {
              await hide;
              if (failHide) throw new Error('Cannot hide video');
            } else if (failRestore) throw new Error('Cannot restore video');
          },
          remainingOcclusion: () => false,
          close: () => {
            events.push('close');
          },
          afterRender: async () => {
            events.push('render');
          },
        },
      );
    let decision = create();
    let shown = false;
    let result = decision.ask(() => {
      shown = true;
    });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    assert(shown && decision.active, `${name}: prompt must be visible`);
    decision.dispose();
    await decision.answer(accepted);
    assert(
      (await result) === cancelled && !decision.active,
      `${name}: disposal must cancel`,
    );
    passed(`${name} decision cancels on disposal and ignores late answers`);

    decision = create();
    failHide = true;
    shown = false;
    let failure = '';
    try {
      await decision.ask(() => {
        shown = true;
      });
    } catch (error) {
      failure = String(error);
    }
    assert(
      failure.includes('Cannot hide video') && !shown && !decision.active,
      `${name}: hide failure must reject without showing or retaining the gate`,
    );
    decision.dispose();
    failHide = false;
    passed(`${name} decision propagates hide failure without exposing content`);

    decision = create();
    hide = new Promise<void>((resolve) => {
      releaseHide = resolve;
    });
    shown = false;
    result = decision.ask(() => {
      shown = true;
    });
    decision.dispose();
    releaseHide!();
    assert(
      (await result) === cancelled && !shown,
      `${name}: late hide cannot show disposed content`,
    );
    hide = null;
    passed(`${name} decision drops a hide reply after disposal`);

    decision = create();
    failRestore = true;
    events.length = 0;
    result = decision.ask(() => {
      events.push('show');
    });
    await new Promise<void>((resolve) => setTimeout(resolve, 0));
    await decision.answer(accepted);
    assert(
      (await result) === cancelled && !decision.active,
      `${name}: restore failure must cancel`,
    );
    assert(
      events.join('|') ===
        'hide|show|close|render|restore|Error: Cannot restore video',
      `${name}: content must close and render before restoring video`,
    );
    decision.dispose();
    passed(
      `${name} decision cancels after restore failure and preserves close ordering`,
    );
  }
  await check<LegacyImportChoice | null>('Legacy import', null, {
    open: true,
    outputPath: '/created.vsfproject',
  });
  await check<'save' | 'discard' | 'cancel'>(
    'Unsaved changes',
    'cancel',
    'save',
  );
  await check<ProjectExitChoice>('Project save failure', 'cancel', 'retry');
  await check<'cancel' | 'choose' | 'reload'>(
    'Project reference',
    'cancel',
    'reload',
  );
}
