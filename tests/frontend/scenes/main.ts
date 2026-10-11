import { mount, unmount, tick } from 'svelte';
import { isTauri } from '@tauri-apps/api/core';
import Harness from './Harness.svelte';
if (isTauri()) throw new Error('Browser fixture only');
let generation = 0;
let running = false;
let mode: 'keyframes' | 'visual' | null = null;
let scanning: 'keyframes' | 'visual' | null = null;
let cuts: number[] = [];
const starts: string[] = [];
const details: string[] = [];
let phase = 'candidates';
let progress = 0;
let cancelled = 0;
let fail = false;
let delay = false;
let release: (() => void) | null = null;
Object.defineProperty(window, '__TAURI_INTERNALS__', {
  configurable: true,
  value: {
    async invoke(
      command: string,
      args: {
        position?: number;
        mode?: 'keyframes' | 'visual';
        generation?: number;
        detail?: string;
      },
    ) {
      if (command === 'scenes_start') {
        if (delay)
          await new Promise<void>((r) => {
            release = r;
          });
        if (fail) throw new Error('Detector unavailable');
        starts.push(args.mode!);
        details.push(args.detail!);
        scanning = args.mode!;
        running = true;
        return ++generation;
      }
      if (command === 'scenes_cancel') {
        cancelled++;
        running = false;
        scanning = null;
        return ++generation;
      }
      if (command === 'scenes_view') {
        if (args.generation !== generation)
          throw new Error('Scene analysis was replaced');
        return {
          generation,
          running,
          mode,
          scanning,
          phase,
          progress,
          partial: running,
          note: null,
          error: null,
          total: cuts.length,
          truncated: false,
          cuts,
          previous: mode === 'visual' && (args.position ?? 0) > 1 ? 1 : null,
          next: mode === 'visual' && (args.position ?? 0) < 2 ? 2 : null,
        };
      }
      throw new Error(command);
    },
  },
});
const host = document.querySelector('#test')!;
function button(text: string) {
  const b = [...host.querySelectorAll('button')].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!b) throw new Error('Missing ' + text);
  return b;
}
async function until(check: () => boolean) {
  const end = performance.now() + 5000;
  await tick();
  while (!check()) {
    if (performance.now() > end) throw new Error('UI timeout');
    await new Promise((r) => setTimeout(r, 20));
  }
  await tick();
}
function assert(ok: boolean, text: string) {
  if (!ok) throw new Error(text);
  const row = document.createElement('li');
  row.textContent = 'PASS: ' + text;
  document.querySelector('#checks')!.append(row);
}
async function run() {
  const app = mount(Harness, { target: host });
  try {
    await tick();
    await new Promise((r) => setTimeout(r, 200));
    await until(() => starts.length === 1);
    assert(
      starts[0] === 'visual' && details[0] === 'standard',
      'Opening media starts the visual pipeline once',
    );
    mode = 'visual';
    cuts = [1];
    phase = 'full';
    progress = 0.42;
    await until(() => !!host.textContent?.includes('Full scan 42%'));
    assert(
      host.querySelector('#cuts')?.textContent === '[]' &&
        !host.querySelector<HTMLInputElement>('input')!.checked,
      'Automatic scan preserves hidden scene preference',
    );
    host.querySelector<HTMLInputElement>('input')!.click();
    await tick();
    assert(
      host.querySelector('progress')?.value === 0.42 &&
        host.querySelector('#cuts')?.textContent === '[1]',
      'Full scan shows percentage, progress bar and live cuts',
    );
    running = false;
    mode = 'visual';
    scanning = null;
    cuts = [1, 2];
    await until(
      () =>
        host.querySelector('#running')?.textContent === 'false' &&
        host.querySelector('#cuts')?.textContent === '[1,2]',
    );
    assert(
      host.querySelector('#cuts')?.textContent === '[1,2]',
      'Completed cuts reach waveform overlay',
    );
    host.querySelector<HTMLInputElement>('input')!.click();
    await tick();
    assert(
      host.querySelector('#cuts')?.textContent === '[]',
      'Visibility toggle hides scene overlay',
    );
    host
      .querySelector<HTMLButtonElement>('[aria-label="Next scene change"]')!
      .click();
    await until(() => host.querySelector('#position')?.textContent === '2');
    await until(
      () =>
        !host.querySelector<HTMLButtonElement>(
          '[aria-label="Previous scene change"]',
        )!.disabled,
    );
    host
      .querySelector<HTMLButtonElement>('[aria-label="Previous scene change"]')!
      .click();
    await tick();
    assert(
      host.querySelector('#position')?.textContent === '1',
      'Scene navigation seeks to neighboring cut',
    );
    host.querySelector<HTMLInputElement>('input')!.click();
    await tick();
    button('Rescan scenes').click();
    await until(() => !!host.textContent?.includes('Full scan'));
    assert(
      host.querySelector('#cuts')?.textContent === '[1,2]',
      'Rescan keeps completed visual cuts visible',
    );
    button('Cancel scan').click();
    await until(
      () =>
        host.querySelector('#running')?.textContent === 'false' &&
        host.querySelector('#cuts')?.textContent === '[1,2]',
    );
    assert(
      cancelled === 1 && host.querySelector('#cuts')?.textContent === '[1,2]',
      'Cancellation preserves completed results',
    );
    await new Promise((r) => setTimeout(r, 300));
    assert(starts.length === 2, 'Cancel does not restart automatic analysis');
    fail = true;
    button('Rescan scenes').click();
    await until(() => !!host.textContent?.includes('Detector unavailable'));
    assert(
      !button('Rescan scenes').disabled &&
        host.querySelector('#cuts')?.textContent === '[1,2]',
      'Failure keeps markers and remains retryable',
    );
    fail = false;
    button('Rescan scenes').click();
    await until(() => !!host.textContent?.includes('Full scan'));
    running = false;
    mode = 'visual';
    scanning = null;
    cuts = [3];
    await until(() => host.querySelector('#cuts')?.textContent === '[3]');
    assert(
      host.querySelector('#cuts')?.textContent === '[3]',
      'Completed rescan replaces visual cuts',
    );
    mode = 'keyframes';
    cuts = [1, 2];
    await until(() => host.querySelector('#cuts')?.textContent === '[]');
    assert(
      host.querySelector<HTMLButtonElement>('[aria-label="Next scene change"]')!
        .disabled,
      'Legacy keyframes never become overlay, snapping or navigation targets',
    );
    generation = 40;
    mode = 'visual';
    cuts = [4];
    button('Restore scenes').click();
    await until(() => host.querySelector('#cuts')?.textContent === '[4]');
    assert(true, 'Restored generation supplies cached visual cuts');
    const detailSelect = host.querySelector<HTMLSelectElement>(
      '[aria-label="Scene scan detail"]',
    )!;
    detailSelect.value = 'reduced';
    detailSelect.dispatchEvent(new Event('change', { bubbles: true }));
    await tick();
    button('Rescan scenes').click();
    await until(() => generation === 41);
    assert(
      details.at(-1) === 'reduced',
      'Reduced detail reaches the typed scan command',
    );
    running = false;
    scanning = null;
    cuts = [5];
    await until(() => host.querySelector('#cuts')?.textContent === '[5]');
    await new Promise((r) => setTimeout(r, 350));
    assert(
      host.querySelector('#cuts')?.textContent === '[5]' &&
        !host.textContent?.includes('Scene analysis was replaced'),
      'Media status polling cannot rewind a rescan to the restored generation',
    );
    delay = true;
    button('Rescan scenes').click();
    await until(() => release !== null);
    const oldRelease = release!;
    delay = false;
    mode = null;
    cuts = [];
    button('Replace media').click();
    await tick();
    oldRelease();
    await new Promise((r) => setTimeout(r, 250));
    assert(
      host.querySelector('#cuts')?.textContent === '[]',
      'Replacement clears previous media markers',
    );
    host.querySelectorAll<HTMLInputElement>('input')[1]!.click();
    await tick();
    const beforeOpen = starts.length;
    button('Replace media').click();
    await new Promise((r) => setTimeout(r, 300));
    assert(
      starts.length === beforeOpen,
      'Automatic scanning can be disabled for newly opened media',
    );
    button('Rescan scenes').click();
    await until(() => starts.length === beforeOpen + 1);
    assert(
      true,
      'Manual scan remains available with automatic scanning disabled',
    );
    document.querySelector('#result')!.textContent =
      'PASS: all 18 scene checks';
  } catch (e) {
    document.querySelector('#result')!.textContent = 'FAIL: ' + String(e);
  } finally {
    await unmount(app);
    Reflect.deleteProperty(window, '__TAURI_INTERNALS__');
  }
}
void run();
