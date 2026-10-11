<script lang="ts">
  import type { TimingSettings } from './magnetic-timing';
  let {
    value,
    disabled,
    onchange,
    onimport,
    onexport,
  }: {
    value: TimingSettings;
    disabled: boolean;
    onchange: (value: TimingSettings) => void;
    onimport: () => void;
    onexport: () => void;
  } = $props();
  function number(
    key: 'cueGapMs' | 'sceneBeforeMs' | 'sceneAfterMs' | 'snapDistancePx',
    event: Event,
  ) {
    const input = event.currentTarget as HTMLInputElement;
    if (input.validity.valid && Number.isInteger(input.valueAsNumber))
      onchange({ ...value, [key]: input.valueAsNumber });
    else input.value = String(value[key]);
  }
</script>

<section class="settings" aria-label="Magnetic timing">
  <h3>Magnetic timing</h3>
  <fieldset {disabled}>
    <label
      ><input
        type="checkbox"
        checked={value.enabled}
        onchange={(e) =>
          onchange({ ...value, enabled: e.currentTarget.checked })}
      /> Enable snapping</label
    >
    <label
      ><input
        type="checkbox"
        checked={value.snapCues}
        onchange={(e) =>
          onchange({ ...value, snapCues: e.currentTarget.checked })}
      /> Cue gaps</label
    >
    <label
      >Between cues (ms) <input
        type="number"
        min="0"
        max="10000"
        step="1"
        value={value.cueGapMs}
        onchange={(e) => number('cueGapMs', e)}
      /></label
    >
    <label
      ><input
        type="checkbox"
        checked={value.snapScenes}
        onchange={(e) =>
          onchange({ ...value, snapScenes: e.currentTarget.checked })}
      /> Scene buffers</label
    >
    <label
      >Before cut (ms) <input
        type="number"
        min="0"
        max="10000"
        step="1"
        value={value.sceneBeforeMs}
        onchange={(e) => number('sceneBeforeMs', e)}
      /></label
    >
    <label
      >After cut (ms) <input
        type="number"
        min="0"
        max="10000"
        step="1"
        value={value.sceneAfterMs}
        onchange={(e) => number('sceneAfterMs', e)}
      /></label
    >
    <label
      >Snap distance (px) <input
        type="number"
        min="1"
        max="32"
        step="1"
        value={value.snapDistancePx}
        onchange={(e) => number('snapDistancePx', e)}
      /></label
    >
    <button type="button" onclick={onimport}>Import timing preset</button>
    <button type="button" onclick={onexport}>Export timing preset</button>
  </fieldset>
  <p>
    Drag a cue edge or a new range near a guide to snap. Hold Alt/Option to
    bypass; arrow-key nudges stay exact. Scene buffers use the displayed
    candidates. Settings guide edits; they do not retime existing cues.
  </p>
</section>

<style>
  .settings {
    font-size: 12px;
    padding: 16px;
    background: #20323f;
  }
  h3 {
    margin: 0 0 12px;
    font-size: 15px;
  }
  fieldset {
    border: 0;
    margin: 6px 0 0;
    padding: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 12px;
    align-items: center;
  }
  label:first-child,
  label:nth-child(4),
  label:nth-child(7) {
    grid-column: 1 / -1;
  }
  @media (max-width: 600px) {
    fieldset {
      grid-template-columns: 1fr;
    }
  }
  label {
    display: flex;
    gap: 5px;
    align-items: center;
  }
  input[type='number'] {
    width: 80px;
  }
  p {
    margin: 6px 0;
    color: #b5c7d3;
  }
</style>
