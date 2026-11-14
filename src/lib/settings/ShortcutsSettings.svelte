<script lang="ts">
  import { actions, type ActionId } from '../workspace/action-registry';
  import {
    shortcutModes,
    shortcutPreset,
    shortcutFromEvent,
    shortcutError,
  } from '../workspace/shortcut-settings';
  import type { ShortcutPreferences, ShortcutMode } from '../client';
  let {
    value,
    disabled,
    onchange,
  }: {
    value: ShortcutPreferences;
    disabled: boolean;
    onchange: (value: ShortcutPreferences) => void;
  } = $props();
  let query = $state('');
  let recording = $state('');
  let recordError = $state('');
  const filtered = $derived(
    actions.filter((action) =>
      [action.label, action.group, ...action.aliases]
        .join(' ')
        .toLocaleLowerCase()
        .includes(query.trim().toLocaleLowerCase()),
    ),
  );
  const conflict = $derived(shortcutError(value));
  const binding = (action: ActionId, mode: ShortcutMode) =>
    value.bindings.find((item) => item.action === action && item.mode === mode)
      ?.key ?? '';
  function change(action: ActionId, mode: ShortcutMode, key: string) {
    const bindings = value.bindings.filter(
      (item) => item.action !== action || item.mode !== mode,
    );
    bindings.push({ action, mode, key: key.trim() });
    onchange({ preset: 'custom', bindings });
  }
  function record(event: KeyboardEvent, action: ActionId, mode: ShortcutMode) {
    if (recording !== `${action}/${mode}`) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.isComposing || event.getModifierState('AltGraph')) {
      recordError =
        'Finish composing text before recording a shortcut. AltGr stays available for typing.';
      recording = '';
      return;
    }
    const key = shortcutFromEvent(event);
    if (key) {
      change(action, mode, key);
      recording = '';
    }
  }
</script>

<section aria-labelledby="shortcuts-settings-title">
  <h3 id="shortcuts-settings-title">Shortcuts</h3>
  <div class="toolbar">
    <label
      >Preset <select
        aria-label="Shortcut preset"
        {disabled}
        value={value.preset}
        onchange={(event) => {
          const preset = event.currentTarget.value;
          if (preset !== 'custom')
            onchange(
              shortcutPreset(
                preset as Exclude<ShortcutPreferences['preset'], 'custom'>,
              ),
            );
        }}
      >
        <option value="current">Current VSF bindings</option><option
          value="vsfDefault">VSF default (trial)</option
        ><option value="visualSubSyncClassic">VisualSubSync classic</option
        >{#if value.preset === 'custom'}<option value="custom">Custom</option
          >{/if}
      </select></label
    >
    <input
      aria-label="Search shortcuts"
      type="search"
      placeholder="Search shortcuts"
      bind:value={query}
    />
  </div>
  <p>
    The proposed VSF default is opt-in for trial. Single keys act only in the
    list or waveform; Tab plays in cue text and Shift+Tab replays. Escape leaves
    text. Other fields keep normal Tab navigation.
  </p>
  {#if value.preset === 'visualSubSyncClassic'}<p>
      Classic maps available VSS actions: F1 play/pause, F2 loop range, Ctrl/⌘+O
      open project, Ctrl/⌘+D shift, Ctrl/⌘+Y redo. VSF cue editing and protected
      navigation remain unchanged; this is a supported mapping, not a complete
      legacy mode.
    </p>{/if}
  <p>
    Type a chord such as CmdOrCtrl+Shift+J, or choose Record and press it. Empty
    removes a binding. Everywhere overlaps both focus modes. Native menus show
    the primary Everywhere binding; text history, Escape, formatting and OS keys
    retain their owners.
  </p>
  {#if conflict}<p role="alert" class="error">
      {conflict}. The conflicting change has not been saved.
    </p>{/if}
  {#if recordError}<p role="alert" class="error">{recordError}</p>{/if}
  <div class="bindings">
    {#each filtered as action (action.id)}
      <div class="binding-row" data-shortcut-action={action.id}>
        <strong>{action.label}<small>{action.group}</small></strong>
        {#each shortcutModes as mode (mode.id)}
          {@const locked =
            action.id === 'quit' || action.id === 'focusWaveform'}
          <label
            >{mode.label}<span>
              <input
                aria-label={`${action.label} — ${mode.label}`}
                disabled={disabled || locked}
                value={binding(action.id, mode.id)}
                placeholder="Unassigned"
                maxlength="48"
                onblur={() => (recording = '')}
                onkeydown={(event) => record(event, action.id, mode.id)}
                onchange={(event) =>
                  change(action.id, mode.id, event.currentTarget.value)}
              />
              <button
                type="button"
                disabled={disabled || locked}
                aria-label={`Record ${action.label} — ${mode.label}`}
                onclick={(event) => {
                  recording = `${action.id}/${mode.id}`;
                  recordError = '';
                  (
                    event.currentTarget
                      .previousElementSibling as HTMLInputElement
                  )?.focus();
                }}
                >{recording === `${action.id}/${mode.id}`
                  ? 'Press…'
                  : 'Record'}</button
              >
            </span></label
          >
        {/each}
      </div>
    {/each}
    {#if !filtered.length}<p>No matching shortcuts.</p>{/if}
  </div>
</section>

<style>
  h3 {
    margin: 0 0 12px;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: center;
  }
  .toolbar label {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  p {
    color: var(--text-muted);
    font-size: 12px;
    line-height: 1.5;
  }
  .error {
    color: var(--err);
  }
  .binding-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    padding: 12px 0;
    border-bottom: 1px solid var(--line);
  }
  strong {
    grid-column: 1 / -1;
    font-size: 13px;
  }
  small {
    color: var(--text-faint);
    margin-left: 8px;
    font-weight: normal;
  }
  label {
    font-size: 11px;
    color: var(--text-muted);
  }
  label span {
    display: flex;
    gap: 4px;
    margin-top: 4px;
  }
  input {
    min-width: 0;
    width: 100%;
  }
  .toolbar input {
    width: 200px;
  }
  button {
    font-size: 11px;
  }
</style>
