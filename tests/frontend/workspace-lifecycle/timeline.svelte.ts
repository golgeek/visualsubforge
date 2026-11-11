import { flushSync } from 'svelte';
import { TimelineView } from '../../../src/lib/timeline-view.svelte';
import { emptyStatus, type WaveformView } from '../../../src/lib/client';

export async function timelineCases(passed: (message: string) => void) {
  const assert = (value: unknown, message: string) => {
    if (!value) throw new Error(message);
  };
  const status = $state({
    ...emptyStatus,
    loaded: true,
    durationSeconds: 100,
    positionSeconds: 10,
  });
  let following = $state(true);
  let gesture = $state(false);
  let owner!: TimelineView;
  let resolve!: (view: WaveformView) => void;
  const destroy = $effect.root(() => {
    owner = new TimelineView(
      {
        status: () => status,
        mediaPath: () => '',
        follow: () => following,
        preview: () => null,
        gesture: () => gesture,
        pending: () => false,
        cues: () => [],
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
    assert(owner.start === 2, 'Playback keeps lookahead');
    status.positionSeconds = 99;
    flushSync();
    assert(owner.start === 80, 'Playback clamps at EOF');
    gesture = true;
    status.positionSeconds = 2;
    flushSync();
    assert(owner.start === 80, 'Gesture suspends following');
    gesture = false;
    flushSync();
    assert(owner.start === 0, 'Loop wrap follows back to the beginning');
    passed(
      'Timeline owner follows playback, clamps EOF and suspends during gestures',
    );
    following = false;
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
