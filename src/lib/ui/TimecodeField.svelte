<script lang="ts">
  import { nudgeTimecode } from './timecode-nudge';
  let {
    label,
    value = $bindable(),
    disabled = false,
    readonly = false,
    frameRate = null,
    timeStepMs,
    invalid = false,
    onerror,
    onkeydown,
  }: {
    label: string;
    value: string;
    disabled?: boolean;
    readonly?: boolean;
    frameRate?: number | null;
    timeStepMs: number;
    invalid?: boolean;
    onerror: (message: string) => void;
    onkeydown?: (event: KeyboardEvent) => void;
  } = $props();
  const hint = $derived(
    frameRate === null
      ? 'Frame rate unavailable; enter an exact time'
      : `One nominal frame at ${frameRate.toFixed(3)} fps; review variable-rate video`,
  );
  function nudge(direction: -1 | 1) {
    if (frameRate === null) return;
    try {
      value = nudgeTimecode(value, direction, frameRate, timeStepMs);
    } catch (error) {
      onerror(error instanceof Error ? error.message : String(error));
    }
  }
</script>

<label>
  <span>{label}</span>
  <span class="field" class:invalid>
    <button
      type="button"
      aria-label={`${label} back one frame`}
      title={hint}
      disabled={disabled || readonly || frameRate === null}
      onmousedown={(event) => event.preventDefault()}
      onclick={() => nudge(-1)}>−</button
    >
    <input
      aria-label={`Cue ${label.toLowerCase()}`}
      bind:value
      {disabled}
      {readonly}
      {onkeydown}
      aria-invalid={invalid}
      spellcheck="false"
    />
    <button
      type="button"
      aria-label={`${label} forward one frame`}
      title={hint}
      disabled={disabled || readonly || frameRate === null}
      onmousedown={(event) => event.preventDefault()}
      onclick={() => nudge(1)}>+</button
    >
  </span>
</label>

<style>
  label,
  .field {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  label {
    gap: 5px;
    font-size: 11px;
  }
  input {
    width: calc(13ch + 14px);
    flex: none;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: 11px;
  }
  button {
    padding: 1px 4px;
    min-width: 18px;
  }
  .invalid input {
    outline: 1px solid var(--err);
  }
</style>
