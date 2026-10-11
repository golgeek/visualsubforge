<script lang="ts">
  import { onMount, tick } from 'svelte';
  import CueTiming from '../../../src/lib/CueTiming.svelte';
  import {
    previewBoundary,
    timingAction,
    type TimingEdit,
  } from '../../../src/lib/cue-timing';
  import type { Cue, DocumentSnapshot } from '../../../src/lib/client';

  const original: Cue = {
    id: 1,
    startMs: 1000,
    endMs: 2000,
    text: 'café 日本語',
  };
  const document: DocumentSnapshot = {
    generation: 1,
    revision: 1,
    loaded: true,
    path: null,
    dirty: false,
    requiresSaveAs: false,
    canUndo: false,
    canRedo: false,
    cues: [original],
  };
  let cue = $state<Cue>({ ...original });
  let mounted = $state(true);
  let enabled = $state(true);
  let width = $state(600);
  let host: HTMLDivElement;
  let edit: TimingEdit | null = null;
  let begins = 0;
  let finishes: boolean[] = [];
  let accepted: Cue[] = [];
  let results = $state<string[]>([]);
  let complete = $state(false);

  function begin() {
    begins++;
    edit = {
      generation: 1,
      revision: 1,
      mediaSession: 1,
      original,
      preview: cue,
    };
    return true;
  }
  function finish(commit: boolean) {
    finishes.push(commit);
    if (commit && edit && timingAction(edit, document, 1, original.id))
      accepted.push({ ...cue });
    edit = null;
    cue = { ...original };
  }
  function assert(condition: boolean, message: string): void {
    if (!condition) throw new Error(message);
  }
  function handle(index = 0): HTMLButtonElement {
    const element = host.querySelectorAll('button')[index];
    if (!element) throw new Error('Missing start handle');
    return element;
  }
  async function key(type: 'keydown' | 'keyup', key: string, repeat = false) {
    handle().dispatchEvent(
      new KeyboardEvent(type, { key, repeat, bubbles: true }),
    );
    await tick();
  }
  async function pointer(type: string, x: number, index = 0, pointerId = 1) {
    const element = handle(index);
    if (type === 'pointerdown') {
      // Synthetic pointers are not active OS pointers. Model capture ownership
      // only; real capture delivery is checked separately in the desktop app.
      let captured = false;
      element.setPointerCapture = () => {
        captured = true;
      };
      element.hasPointerCapture = () => captured;
      element.releasePointerCapture = () => {
        captured = false;
      };
    }
    element.dispatchEvent(
      new PointerEvent(type, {
        pointerId,
        isPrimary: true,
        button: 0,
        clientX: x,
        bubbles: true,
      }),
    );
    await tick();
  }
  function grab(index = 0) {
    const rect = handle(index).getBoundingClientRect();
    return rect.right - 3;
  }
  async function paint() {
    await tick();
    // Two frames allow the real ResizeObserver to deliver initial/changed layout.
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
    );
  }
  async function reset() {
    mounted = false;
    await tick();
    cue = { ...original };
    begins = 0;
    finishes = [];
    accepted = [];
    enabled = true;
    mounted = true;
    await paint();
  }
  async function run() {
    try {
      await paint();
      await key('keydown', 'ArrowRight');
      await key('keydown', 'ArrowRight', true);
      await key('keydown', 'ArrowRight', true);
      assert(
        cue.startMs === 1030 && accepted.length === 0,
        'Held key must preview three steps without accepting',
      );
      await key('keyup', 'ArrowRight');
      await key('keyup', 'ArrowRight');
      assert(
        begins === 1 && finishes.length === 1 && finishes[0] === true,
        'Held key must finish exactly once',
      );
      assert(
        accepted[0]?.startMs === 1030 && accepted[0]?.endMs === 2000,
        'Final accepted boundary must include every repeat',
      );
      results.push(
        'PASS: held-arrow repeats preview three steps and commit once',
      );

      for (const cancellation of [
        'Escape',
        'handle blur',
        'window blur',
        'resize',
        'unmount',
      ]) {
        await reset();
        await key('keydown', 'ArrowRight');
        assert(
          cue.startMs === 1010,
          'Cancellation fixture must contain a preview',
        );
        if (cancellation === 'Escape') await key('keydown', 'Escape');
        else if (cancellation === 'handle blur')
          handle().dispatchEvent(new FocusEvent('blur'));
        else if (cancellation === 'window blur')
          window.dispatchEvent(new Event('blur'));
        else if (cancellation === 'resize') width -= 10;
        else mounted = false;
        await paint();
        if (mounted) {
          // A cancelled held key may keep producing repeats until physical release.
          await key('keydown', 'ArrowRight', true);
          await key('keyup', 'ArrowRight');
        }
        assert(
          finishes.length === 1 && finishes[0] === false,
          `${cancellation}: exactly one cancelled finish required`,
        );
        assert(
          accepted.length === 0 && cue.startMs === original.startMs,
          `${cancellation}: must preserve accepted timing`,
        );
        results.push(
          `PASS: ${cancellation} cancels without a later keyup commit`,
        );
      }
      await reset();
      enabled = false;
      await tick();
      await key('keydown', 'ArrowRight');
      await key('keyup', 'ArrowRight');
      assert(
        begins === 0 && finishes.length === 0 && cue.startMs === 1000,
        'Disabled handle must not begin or commit',
      );
      results.push('PASS: disabled handle rejects keyboard gestures');
      await reset();
      let x = grab();
      await pointer('pointerdown', x);
      await pointer('pointermove', x);
      await pointer('pointerup', x);
      assert(
        cue.startMs === 1000 && accepted.length === 0,
        'Off-centre stationary click must preserve exact timing',
      );
      results.push('PASS: off-centre stationary click preserves timing');

      await reset();
      x = grab();
      await pointer('pointerdown', x);
      await pointer('pointermove', x + 1);
      await pointer('pointerup', x + 2);
      assert(
        accepted.length === 0,
        'Sub-threshold pointer jitter must not retime',
      );
      results.push('PASS: focusing with pointer jitter creates no edit');

      await reset();
      x = grab();
      const delta = (width * 0.4) / 5;
      await pointer('pointerdown', x);
      await pointer('pointermove', x + delta);
      assert(
        cue.startMs === 1400 && accepted.length === 0,
        'Dragging must preserve grab offset and preview without committing',
      );
      await pointer('pointermove', x);
      await pointer('pointerup', x);
      assert(
        accepted.length === 0 && finishes.length === 1,
        'Returning to the original pointer position must create no edit',
      );
      results.push(
        'PASS: relative drag previews and exact return creates no edit',
      );

      await reset();
      x = grab();
      await pointer('pointerdown', x);
      await pointer('pointermove', x + delta);
      await pointer('pointerup', x + delta * 2);
      await pointer('pointerup', x + delta * 2);
      assert(
        accepted.length === 1 &&
          accepted[0]?.startMs === 1800 &&
          finishes.length === 1,
        'Final release coordinate must commit exactly once',
      );
      results.push(
        'PASS: release includes final pointer position and commits once',
      );

      await reset();
      x = grab(1);
      await pointer('pointerdown', x, 1);
      await pointer('pointerup', x + (width * 0.5) / 5, 1);
      assert(
        accepted.length === 1 &&
          accepted[0]?.endMs === 2500 &&
          accepted[0]?.startMs === 1000,
        'End handle release must move only the end boundary',
      );
      results.push('PASS: end handle uses relative release position');

      await reset();
      await key('keydown', 'ArrowRight');
      await key('keyup', 'ArrowRight');
      x = grab(1);
      await pointer('pointerdown', x, 1);
      assert(
        finishes.length === 1,
        'Blurring the previously focused start handle must not cancel the new end gesture',
      );
      await pointer('pointerup', x + (width * 0.5) / 5, 1);
      assert(
        accepted.length === 2 && accepted[1]?.endMs === 2500,
        'Switching handle focus must allow the first drag to commit',
      );
      results.push('PASS: switching handles preserves the new gesture');

      for (const cancellation of ['pointercancel', 'lostpointercapture']) {
        for (const foreign of ['pointer', 'handle', 'keyboard']) {
          await reset();
          x = grab();
          if (foreign === 'keyboard') await key('keydown', 'ArrowRight');
          else {
            await pointer('pointerdown', x);
            await pointer('pointermove', x + delta);
          }
          await pointer(
            cancellation,
            x + delta,
            foreign === 'handle' ? 1 : 0,
            foreign === 'pointer' ? 2 : 1,
          );
          assert(
            finishes.length === 0,
            `${cancellation}: unrelated ${foreign} must not cancel the active gesture`,
          );
          if (foreign === 'keyboard') await key('keyup', 'ArrowRight');
          else await pointer('pointerup', x + delta * 2);
          assert(
            finishes.length === 1 &&
              finishes[0] === true &&
              accepted.length === 1 &&
              accepted[0]?.startMs === (foreign === 'keyboard' ? 1010 : 1800),
            `${cancellation}: active ${foreign} fixture must still commit exactly once`,
          );
          results.push(
            foreign === 'keyboard'
              ? `PASS: ${cancellation} preserves a held-key gesture`
              : `PASS: ${cancellation} ignores an unrelated ${foreign} event`,
          );
        }
        await reset();
        x = grab();
        await pointer('pointerdown', x);
        await pointer('pointermove', x + delta);
        await pointer(cancellation, x + delta);
        await pointer('pointermove', x + delta * 2);
        await pointer('pointerup', x + delta * 2);
        assert(
          accepted.length === 0 &&
            finishes.length === 1 &&
            finishes[0] === false,
          `${cancellation}: no commit after cancelled pointer gesture`,
        );
        results.push(
          `PASS: ${cancellation} rejects later pointer movement/release`,
        );
      }
      complete = true;
    } catch (error) {
      results.push(`FAIL: ${String(error)}`);
    }
  }
  onMount(() => {
    void run();
  });
</script>

<h1>Cue timing interaction regression</h1>
<p>
  Actual Svelte component; synthetic keyboard/pointer/focus events and real
  layout observer. No native input, Rust history, or audio validation is
  claimed.
</p>
<div
  bind:this={host}
  style:width={`${width}px`}
  style="position: relative; height: 100px; background: #222;"
>
  {#if mounted}
    <CueTiming
      {cue}
      start={0}
      span={5}
      duration={60}
      {enabled}
      onbegin={begin}
      onpreview={(boundary, milliseconds) => {
        if (!edit) throw new Error('Preview without a gesture');
        edit = previewBoundary(edit, boundary, milliseconds);
        cue = edit.preview;
      }}
      onfinish={finish}
    />
  {/if}
</div>
<ul>
  {#each results as result (result)}<li>{result}</li>{/each}
</ul>
<p role="status">
  {complete ? 'PASS: all 21 interaction checks' : 'Incomplete'}
</p>
