<script lang="ts">
  import { onMount } from 'svelte';
  import CueTiming from '../../../src/lib/CueTiming.svelte';
  import {
    previewBoundary,
    type TimingEdit,
  } from '../../../src/lib/cue-timing';
  import type { Cue } from '../../../src/lib/client';

  let accepted = $state<Cue>({
    id: 1,
    startMs: 1000,
    endMs: 4000,
    text: 'Input routing fixture',
  });
  let edit = $state<TimingEdit | null>(null);
  let commits = $state(0);
  let cancellations = $state(0);
  let events = $state<string[]>([]);
  const cue = $derived(edit?.preview ?? accepted);

  onMount(() => {
    function record(event: KeyboardEvent) {
      // Observe after the component handler, without changing or synthesizing input.
      queueMicrotask(() => {
        events = [
          `${event.type} ${event.key} code=${event.code} alt=${event.altKey} ctrl=${event.ctrlKey} meta=${event.metaKey} shift=${event.shiftKey} repeat=${event.repeat} prevented=${event.defaultPrevented}`,
          ...events,
        ].slice(0, 20);
      });
    }
    window.addEventListener('keydown', record, true);
    window.addEventListener('keyup', record, true);
    return () => {
      window.removeEventListener('keydown', record, true);
      window.removeEventListener('keyup', record, true);
    };
  });
</script>

<h1>Native input routing diagnostic</h1>
<p>Focus S or E. Arrows: 10 ms; Shift: 100 ms; Alt: 1 ms. Escape cancels.</p>
<p>
  This mounts the production control with local fixture state. It does not call
  Tauri, save files or validate Rust history or audio.
</p>
<p data-testid="accepted">
  Accepted: {accepted.startMs}–{accepted.endMs} ms. Commits: {commits}.
  Cancellations: {cancellations}.
</p>
<p data-testid="preview">Preview: {cue.startMs}–{cue.endMs} ms.</p>
<div style="position: relative; height: 110px; width: 700px; background: #222;">
  <CueTiming
    {cue}
    start={0}
    span={5}
    duration={60}
    enabled={true}
    onbegin={() => {
      edit = {
        generation: 1,
        revision: commits,
        mediaSession: 1,
        original: accepted,
        preview: accepted,
      };
      return true;
    }}
    onpreview={(boundary, milliseconds) => {
      if (edit) edit = previewBoundary(edit, boundary, milliseconds);
    }}
    onfinish={(commit) => {
      if (!edit) return;
      if (commit) {
        if (
          accepted.startMs !== edit.preview.startMs ||
          accepted.endMs !== edit.preview.endMs
        ) {
          accepted = edit.preview;
          commits++;
        }
      } else cancellations++;
      edit = null;
    }}
  />
</div>
<h2>Most recent keyboard events</h2>
<pre>{events.join('\n')}</pre>
