<script lang="ts">
  import { onMount } from 'svelte';
  import SceneControls from '../../../src/lib/SceneControls.svelte';
  let media = $state({ session: 1, loaded: true });
  let restored = $state<{ session: number; generation: number } | null>(null);
  onMount(() => {
    const timer = setInterval(() => {
      media = { ...media };
    }, 50);
    return () => clearInterval(timer);
  });
  let controls: {
    rescan: () => Promise<void>;
    cancel: () => Promise<void>;
    navigate: (backwards: boolean) => void;
  };
  let scanState = $state({
    running: false,
    pending: false,
    previous: false,
    next: false,
  });
  let detail = $state<'standard' | 'reduced'>('standard');
  let autoScan = $state(true);
  let shown = $state(false);
  let cuts = $state<number[]>([]);
  let position = $state(0);
</script>

<button
  onclick={() => {
    media = { session: media.session + 1, loaded: true };
    restored = null;
  }}>Replace media</button
>
<button
  onclick={() => {
    restored = { session: media.session, generation: 40 };
  }}>Restore scenes</button
>
<button onclick={() => void controls.rescan()}>Rescan scenes</button>
<button onclick={() => void controls.cancel()}>Cancel scan</button>
<label><input type="checkbox" bind:checked={shown} /> Scenes</label>
<label><input type="checkbox" bind:checked={autoScan} /> Auto scan</label>
<select aria-label="Scene scan detail" bind:value={detail}
  ><option value="standard">Standard</option><option value="reduced"
    >Reduced</option
  ></select
>
<button
  aria-label="Previous scene change"
  disabled={!scanState.previous}
  onclick={() => controls.navigate(true)}>Previous</button
>
<button
  aria-label="Next scene change"
  disabled={!scanState.next}
  onclick={() => controls.navigate(false)}>Next</button
>
<output id="running">{String(scanState.running)}</output>
<SceneControls
  bind:this={controls}
  onstate={(value) => (scanState = value)}
  {detail}
  {autoScan}
  path="/fixture.mkv"
  session={media.session}
  loaded={media.loaded}
  {restored}
  bind:shown
  start={0}
  span={10}
  {position}
  disabled={false}
  oncuts={(value) => {
    cuts = value;
  }}
  onseek={(value) => {
    position = value;
  }}
/>
<output id="cuts">{JSON.stringify(cuts)}</output>
<output id="position">{position}</output>
