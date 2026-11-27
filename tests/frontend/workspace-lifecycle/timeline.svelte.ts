import { flushSync } from 'svelte';
import { TimelineView } from '../../../src/lib/timeline/timeline-view.svelte';
import { emptyStatus, type WaveformView } from '../../../src/lib/client';

export async function timelineCases(passed: (message: string) => void) {
  const assert = (value: unknown, message: string) => {
    if (!value) throw new Error(message);
  };
  let status = $state({
    ...emptyStatus,
    loaded: true,
    durationSeconds: 100,
    positionSeconds: 10,
  });
  let owner!: TimelineView;
  let resolve!: (view: WaveformView) => void;
  const destroy = $effect.root(() => {
    owner = new TimelineView(
      {
        status: () => status,
        mediaPath: () => '',
        preview: () => null,
        pending: () => false,
        cues: () => [{ id: 1, startMs: 3000, endMs: 4000, text: 'A cue' }],
        gap: () => 0,
        selectRange: () => {},
      },
      async () => 1,
      () =>
        new Promise((yes) => {
          resolve = yes;
        }),
    );
  });
  try {
    flushSync();
    const first = owner.waveCues;
    assert(
      owner.waveCues === first,
      'Repeated reads reuse the visible cue projection',
    );
    const preview = { ...first[0]!, endMs: 5000 };
    owner.connectPreview(() => preview);
    assert(
      owner.waveCues[0] === preview,
      'Connecting a preview invalidates the cached cues',
    );
    owner.connectPreview(() => null);
    assert(owner.start === 2, 'Playback keeps lookahead');
    status.positionSeconds = 99;
    flushSync();
    assert(
      first.length === 1 && owner.waveCues.length === 0,
      'Panning updates the visible cues',
    );
    assert(owner.start === 80, 'Playback clamps at EOF');
    owner.interacting = true;
    status.positionSeconds = 2;
    flushSync();
    assert(owner.start === 80, 'Gesture suspends following');
    owner.interacting = false;
    flushSync();
    assert(owner.start === 0, 'Loop wrap follows back to the beginning');
    passed(
      'Timeline owner follows playback, clamps EOF and suspends during gestures',
    );
    owner.suspended = true;
    owner.start = 40;
    status.positionSeconds = 20;
    flushSync();
    assert(
      owner.follow && owner.start === 40,
      'Panning suspends without disabling preference',
    );
    owner.zoom(0.5);
    assert(
      owner.follow && owner.span === 10 && owner.start === 45,
      'Suspended button zoom stays centered in the viewed region',
    );
    owner.navigate({ start: 47, span: 5 });
    assert(
      owner.start === 47 && owner.span === 5 && owner.suspended,
      'Suspended wheel zoom retains the pointer-anchored view away from the playhead',
    );
    owner.follow = false;
    owner.suspended = false;
    owner.navigate({ start: 48, span: 2.5 });
    assert(
      owner.start === 48 && owner.span === 2.5 && !owner.follow,
      'Explicitly disabled following retains the wheel anchor',
    );
    owner.follow = true;
    owner.suspended = true;
    status.paused = false;
    flushSync();
    assert(
      !owner.suspended && owner.follow,
      'Starting playback resumes temporary suspension',
    );
    owner.navigate({ start: 48, span: 10 });
    assert(
      owner.start === 15 && owner.span === 10,
      'Active following zooms around the playhead rather than the pointer',
    );
    flushSync();
    owner.suspended = true;
    owner.start = 40;
    status = { ...status, positionSeconds: 21 };
    flushSync();
    assert(
      owner.suspended && owner.start === 40,
      'Playing status polls do not resume a user pan',
    );
    status.paused = true;
    owner.frameCue({ id: 1, startMs: 50000, endMs: 56000, text: '' });
    flushSync();
    assert(
      owner.span === 10 && owner.start === 48,
      'Cue occupies sixty percent of centered view',
    );
    owner.frameCue({ id: 2, startMs: 80000, endMs: 81000, text: '' }, false);
    flushSync();
    assert(
      owner.span === 10 && owner.start === 75.5,
      'Disabling cue zoom still centers without changing scale',
    );
    owner.frameCue({ id: 3, startMs: 80000, endMs: 81000, text: '' });
    flushSync();
    assert(
      owner.span === 8 && owner.start === 76.5,
      'Short cues retain eight seconds of neighboring context',
    );
    owner.frameCue({ id: 4, startMs: 0, endMs: 1000, text: '' });
    flushSync();
    assert(
      owner.span === 8 && owner.start === 0,
      'Cue framing clamps at media start',
    );
    owner.frameCue({ id: 5, startMs: 99000, endMs: 100000, text: '' });
    flushSync();
    assert(
      owner.span === 8 && owner.start === 92,
      'Cue framing clamps at media end',
    );
    owner.resumeFollowing();
    flushSync();
    assert(!owner.suspended, 'Jump resumes following');
    passed(
      'Panning, playback resume and optional cue framing preserve explicit following',
    );
    owner.follow = false;
    flushSync();
    owner.zoom(100);
    assert(owner.span === 100 && owner.start === 0, 'Zoom out clamps to media');
    owner.zoom(0.00001);
    assert(
      owner.span === 0.1 && owner.start >= 0,
      'Zoom in clamps to minimum span',
    );
    passed('Timeline owner clamps zoom at media and minimum span');
    owner.generation = 1;
    const old = owner.poll();
    owner.reset();
    resolve({} as WaveformView);
    await old;
    assert(owner.wave === null, 'Reset drops a stale waveform reply');
    owner.generation = 2;
    const disposed = owner.poll();
    owner.dispose();
    resolve({} as WaveformView);
    await disposed;
    assert(owner.wave === null, 'Disposal drops a stale waveform reply');
    passed('Timeline owner drops waveform replies after reset and disposal');
  } finally {
    owner.dispose();
    destroy();
  }
}
