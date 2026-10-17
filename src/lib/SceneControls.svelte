<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import {
    scenesStart,
    scenesCancel,
    scenesView,
    type SceneView,
    type SceneDetail,
  } from './client';
  let {
    path,
    session,
    loaded,
    start,
    span,
    position,
    disabled,
    oncuts,
    onseek,
    padding = 0,
    restored = null,
    shown = $bindable(true),
    autoScan = true,
    detail = 'standard',
    onstate = () => {},
    onfinish = () => {},
  }: {
    onfinish?: (message: string) => void;
    autoScan?: boolean;
    detail?: SceneDetail;
    onstate?: (state: {
      qualityKey: string;
      running: boolean;
      pending: boolean;
      previous: boolean;
      next: boolean;
    }) => void;
    padding?: number;
    restored?: { session: number; generation: number } | null;
    shown?: boolean;
    path: string;
    session: number;
    loaded: boolean;
    start: number;
    span: number;
    position: number | null;
    disabled: boolean;
    oncuts: (cuts: number[]) => void;
    onseek: (seconds: number) => void;
  } = $props();
  let generation = $state<number | null>(null);
  let view = $state<SceneView | null>(null);
  let pending = $state(false);
  let error = $state('');
  let epoch = 0;
  let alive = true;
  let mediaKey = '';
  let restoredKey = '';
  let polling = false;
  let autoStarted = false;
  const phaseLabel = $derived(
    view?.phase === 'full'
      ? 'Full scan'
      : view?.phase === 'verifying'
        ? 'Checking candidates'
        : 'Finding candidates',
  );
  const percent = $derived(
    Math.floor(Math.max(0, Math.min(1, view?.progress ?? 0)) * 100),
  );
  const message = $derived(
    error ||
      view?.error ||
      (view?.running
        ? `${phaseLabel}${view.phase === 'candidates' ? '…' : ` ${percent}%`} · ${view.total} cuts`
        : view?.mode === 'visual'
          ? `${view.total} visual cuts${view.partial ? ' · partial' : ''}${view.truncated ? ' · zoom for all' : ''}`
          : ''),
  );
  $effect(() => {
    const key = `${session}:${loaded}:${path}`;
    if (mediaKey === key) return;
    mediaKey = key;
    restoredKey = '';
    autoStarted = false;
    ++epoch;
    generation = null;
    view = null;
    pending = false;
    error = '';
    // Rust cancels/reaps the preceding job when media is opened/closed.
  });
  $effect(() => {
    oncuts(shown && view?.mode === 'visual' ? view.cuts : []);
  });
  $effect(() => {
    if (restored && restored.session === session && loaded) {
      // Parent media snapshots are replaced during playback polling. Adopt each
      // restored cache once, so those updates cannot rewind a newer scan job.
      const key = `${restored.session}:${restored.generation}`;
      if (restoredKey === key) return;
      restoredKey = key;
      autoStarted = true;
      ++epoch;
      pending = false;
      generation = restored.generation;
      untrack(() => void poll());
    }
  });
  $effect(() => {
    // Wait for the parent's open/restore transaction to finish. A completed
    // project cache suppresses automatic work; Cancel never triggers a restart.
    if (loaded && path && !disabled) {
      untrack(() => {
        if (!autoStarted && !(restored && restored.session === session)) {
          autoStarted = true;
          if (autoScan) void detect(true);
        }
      });
    }
  });
  $effect(() => {
    onstate({
      qualityKey: `${generation}/${view?.running}/${view?.partial}/${view?.mode}/${view?.error}`,
      running: !!view?.running,
      pending,
      previous: view?.previous != null,
      next: view?.next != null,
    });
  });
  let wasRunning = false;
  $effect(() => {
    const running = !!view?.running;
    if (wasRunning && !running && view?.mode === 'visual') onfinish(message);
    wasRunning = running;
  });
  export async function rescan() {
    if (!view?.running) await detect();
  }
  export async function cancel() {
    if (view?.running) await detect();
  }
  export function navigate(backwards: boolean) {
    const target = backwards ? view?.previous : view?.next;
    if (!disabled && target != null) onseek(target);
  }
  async function detect(automatic = false) {
    if (!loaded || !path || pending || disabled) return;
    const request = ++epoch;
    pending = true;
    error = '';
    try {
      if (view?.running) {
        const next = await scenesCancel(session);
        if (alive && request === epoch) {
          generation = next;
          if (view)
            view = { ...view, running: false, scanning: null, error: null };
        }
      } else {
        const next = await scenesStart(path, session, 'visual', detail);
        if (alive && request === epoch) {
          generation = next;
          if (!automatic) shown = true;
          view = {
            mode: view?.mode ?? null,
            scanning: 'visual',
            phase: 'candidates',
            progress: 0,
            partial: true,
            note: null,
            generation: next,
            running: true,
            error: null,
            total: view?.total ?? 0,
            truncated: false,
            cuts: view?.cuts ?? [],
            previous: view?.previous ?? null,
            next: view?.next ?? null,
          };
        }
      }
    } catch (e) {
      if (alive && request === epoch) error = String(e);
    } finally {
      if (alive && request === epoch) pending = false;
    }
  }
  async function poll() {
    if (generation === null || polling || pending) return;
    polling = true;
    const request = epoch;
    try {
      const next = await scenesView(
        generation,
        Math.max(0, start - padding),
        start + span + padding,
        position ?? 0,
      );
      if (alive && request === epoch) view = next;
    } catch (e) {
      if (alive && request === epoch) {
        error = String(e);
        generation = null;
        view = null;
      }
    } finally {
      polling = false;
    }
  }
  onMount(() => {
    const timer = setInterval(() => void poll(), 150);
    return () => {
      alive = false;
      ++epoch;
      clearInterval(timer);
    };
  });
</script>

{#if view?.running || error || view?.error}
  <div class="scenes" aria-label="Scene detection">
    {#if view?.running}
      <progress
        aria-label={phaseLabel}
        max="1"
        value={view.phase === 'candidates' ? undefined : view.progress}
      ></progress>
    {/if}
    <span
      class="status"
      role="status"
      title={[message, view?.note].filter(Boolean).join(' · ')}>{message}</span
    >
  </div>
{/if}

<style>
  .scenes {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    font-size: 11px;
  }
  progress {
    width: 75px;
    height: 8px;
    flex-shrink: 0;
    accent-color: #b794f4;
  }
  .status {
    max-width: 230px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
