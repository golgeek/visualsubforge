<script lang="ts">
  import type { QualityProfile } from './client';
  import {
    copyProfile,
    customizeProfile,
    defaultLanguageTools,
  } from './quality-profile';
  import LanguageNotices from './LanguageNotices.svelte';
  let {
    value,
    builtins,
    disabled,
    onchange,
    onimport,
    onexport,
  }: {
    value: QualityProfile;
    builtins: QualityProfile[];
    disabled: boolean;
    onchange: (profile: QualityProfile) => void;
    onimport: () => void;
    onexport: () => void;
  } = $props();
  let editableId = $state<string | null>(null);
  const readOnly = $derived(disabled || editableId !== value.id);
  const language = $derived(value.languageTools ?? defaultLanguageTools);
  const languageRules = [
    ['spelling', 'Check spelling'],
    ['typography', 'Check typography'],
    ['capitalization', 'Check conservative sentence capitalization'],
    ['dialogueMarkers', 'Check existing dialogue markers'],
  ] as const;
  const speeds = [
    ['minimumRs', 'Minimum RS (inclusive)'],
    ['maximumRsExclusive', 'Maximum RS (exclusive)'],
    ['minimumCps', 'Minimum CPS'],
    ['maximumCps', 'Maximum CPS'],
  ] as const;
  function edit(change: (profile: QualityProfile) => void) {
    if (readOnly) return;
    const draft = copyProfile(value);
    change(draft);
    onchange(draft);
  }
  function customize() {
    const draft = customizeProfile(value);
    editableId = draft.id;
    onchange(draft);
  }
</script>

<section aria-labelledby="quality-settings-title">
  <h3 id="quality-settings-title">Subtitle quality</h3>
  <fieldset {disabled}>
    <label
      >Profile
      <select
        aria-label="Quality profile"
        value={value.id}
        onchange={(e) => {
          const profile = builtins.find((p) => p.id === e.currentTarget.value);
          if (profile) {
            editableId = null;
            onchange(copyProfile(profile));
          }
        }}
      >
        {#if !builtins.some((p) => p.id === value.id)}<option value={value.id}
            >{value.name}</option
          >{/if}
        {#each builtins as profile (profile.id)}<option value={profile.id}
            >{profile.name}</option
          >{/each}
      </select>
    </label>
    <div class="actions">
      <button type="button" onclick={onimport}>Import quality profile</button>
      <button type="button" onclick={onexport}>Export quality profile</button>
      <button type="button" onclick={customize}>Customize a copy</button>
    </div>
  </fieldset>
  <p class="coverage" role="status">
    {value.limitations.length
      ? 'Partial coverage — unsupported rules are not checked.'
      : 'Checks all enabled rules in this profile.'}
  </p>
  {#if value.limitations.length}<ul>
      {#each value.limitations as limitation, index (index)}<li>
          {limitation}
        </li>{/each}
    </ul>{/if}
  <details>
    <summary>Profile source and counting policy</summary>
    <p>{value.publisher} · {value.language} · revision {value.revision}</p>
    <p>{value.source}</p>
    <p>
      Counting: {value.textAlgorithm}. Complete HTML and ASS/SSA blocks are
      excluded. UTF-16 units count characters; retained physical breaks count as
      two units. ASS text escapes and drawing text are counted literally. These
      are source line lengths, not rendered wrapping or pixel widths.
    </p>
    <p>
      RS uses a 500 ms reading allowance. Empty text and short durations can
      make speed unavailable. Display rounding never changes a rule decision.
    </p>
  </details>
  <fieldset disabled={readOnly} class="rules">
    <label class="wide"
      >Language dictionary
      <select
        aria-label="Language dictionary"
        value={language.dictionary}
        onchange={(e) =>
          edit((p) => {
            p.languageTools = {
              ...language,
              dictionary: e.currentTarget.value as typeof language.dictionary,
            };
          })}
      >
        <option value="en-US">English (US) · SCOWL 2020.12.07</option>
        <option value="fr">French Classique · Grammalecte 7.7</option>
      </select>
    </label>
    {#each languageRules as [key, label] (key)}
      <label class="wide"
        ><input
          type="checkbox"
          checked={language[key]}
          onchange={(e) =>
            edit((p) => {
              p.languageTools = { ...language, [key]: e.currentTarget.checked };
            })}
        />
        {label}</label
      >
    {/each}
    <label class="wide"
      >Custom profile name <input
        required
        maxlength="120"
        value={value.name}
        oninput={(e) => edit((p) => (p.name = e.currentTarget.value))}
      /></label
    >
    <label class="wide"
      >Speed calculation
      <select
        value={value.speedBasis}
        onchange={(e) =>
          edit(
            (p) =>
              (p.speedBasis = e.currentTarget
                .value as QualityProfile['speedBasis']),
          )}
      >
        <option value="ordinary_v1">Cue duration</option>
        <option value="vss_overlap_v1">VSS overlap-adjusted duration</option>
      </select>
    </label>
    <label class="wide"
      ><input
        type="checkbox"
        checked={value.rules.duration.enabled}
        onchange={(e) =>
          edit((p) => (p.rules.duration.enabled = e.currentTarget.checked))}
      /> Check duration</label
    >
    <label
      >Minimum duration (ms)<input
        type="number"
        required
        min="1"
        max="3600000"
        step="1"
        value={value.rules.duration.minimumMs}
        onchange={(e) =>
          edit(
            (p) => (p.rules.duration.minimumMs = Number(e.currentTarget.value)),
          )}
      /></label
    >
    <label
      >Maximum duration (ms)<input
        type="number"
        required
        min="1"
        max="3600000"
        step="1"
        value={value.rules.duration.maximumMs}
        onchange={(e) =>
          edit(
            (p) => (p.rules.duration.maximumMs = Number(e.currentTarget.value)),
          )}
      /></label
    >
    <label class="wide"
      ><input
        type="checkbox"
        checked={value.rules.speed.enabled}
        onchange={(e) =>
          edit((p) => (p.rules.speed.enabled = e.currentTarget.checked))}
      /> Check speed</label
    >
    {#each speeds as [key, label] (key)}
      <label
        >{label}<input
          type="number"
          min="1"
          max="1000"
          step="1"
          value={value.rules.speed[key] ?? ''}
          placeholder="Off"
          onchange={(e) =>
            edit(
              (p) =>
                (p.rules.speed[key] =
                  e.currentTarget.value === ''
                    ? null
                    : Number(e.currentTarget.value)),
            )}
        /></label
      >
    {/each}
    <p class="wide">
      A blank speed threshold disables that bound. CPS bounds are inclusive.
    </p>
    <label class="wide"
      ><input
        type="checkbox"
        checked={value.rules.lines.enabled}
        onchange={(e) =>
          edit((p) => (p.rules.lines.enabled = e.currentTarget.checked))}
      /> Check line count and length</label
    >
    <label
      >Maximum lines<input
        type="number"
        required
        min="1"
        max="100"
        step="1"
        value={value.rules.lines.maximumLines}
        onchange={(e) =>
          edit(
            (p) => (p.rules.lines.maximumLines = Number(e.currentTarget.value)),
          )}
      /></label
    >
    <label
      >Preferred single-line characters<input
        type="number"
        required
        min="1"
        max="10000"
        step="1"
        value={value.rules.lines.singleLinePreferred}
        onchange={(e) =>
          edit(
            (p) =>
              (p.rules.lines.singleLinePreferred = Number(
                e.currentTarget.value,
              )),
          )}
      /></label
    >
    <label
      >Maximum characters per line<input
        type="number"
        required
        min="1"
        max="10000"
        step="1"
        value={value.rules.lines.maximumCharacters}
        onchange={(e) =>
          edit(
            (p) =>
              (p.rules.lines.maximumCharacters = Number(e.currentTarget.value)),
          )}
      /></label
    >
    <label class="wide"
      ><input
        type="checkbox"
        checked={value.rules.gaps}
        onchange={(e) => edit((p) => (p.rules.gaps = e.currentTarget.checked))}
      /> Check overlaps and minimum gaps</label
    >
    <label class="wide"
      ><input
        type="checkbox"
        checked={value.rules.empty}
        onchange={(e) => edit((p) => (p.rules.empty = e.currentTarget.checked))}
      /> Check empty cues</label
    >
  </fieldset>
  <p>
    Gap and scene thresholds below are shared with this profile. Changing them
    creates a custom copy and preserves the other rules. Apply never retimes
    cues.
  </p>
  <p>
    Typography covers apostrophes, ellipses and language-specific punctuation
    spaces. Capitalization checks the document start and text after ! or ?,
    including across cues; periods remain ambiguous. Dialogue checks normalize
    existing markers only.
  </p>
  <LanguageNotices />
</section>

<style>
  section {
    padding: 16px;
    background: #20323f;
    font-size: 12px;
  }
  h3 {
    margin: 0 0 12px;
    font-size: 15px;
  }
  fieldset {
    border: 0;
    padding: 0;
    margin: 8px 0;
    min-width: 0;
  }
  label {
    display: flex;
    gap: 8px;
    align-items: center;
    justify-content: space-between;
  }
  label:has(input[type='checkbox']) {
    justify-content: flex-start;
  }
  select,
  input:not([type='checkbox']) {
    min-width: 0;
  }
  select {
    max-width: 75%;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 12px;
  }
  .rules {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px 18px;
  }
  .wide {
    grid-column: 1 / -1;
  }
  input[type='number'] {
    width: 80px;
  }
  .coverage {
    font-weight: 600;
  }
  p,
  li {
    color: #c2d3df;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }
  summary {
    cursor: pointer;
    margin: 12px 0;
  }
  @media (max-width: 600px) {
    .rules {
      grid-template-columns: 1fr;
    }
  }
</style>
