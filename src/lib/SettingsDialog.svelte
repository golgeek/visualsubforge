<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import TimingSettingsPanel from './TimingSettings.svelte';
  import {
    chooseFile,
    importTimingPreset,
    exportTimingPreset,
    savePreferences,
    type Preferences,
  } from './client';
  import type { TimingSettings } from './magnetic-timing';
  let {
    preferences,
    timing,
    onapply,
    onclose,
  }: {
    preferences: Preferences;
    timing: TimingSettings;
    onapply: (value: Preferences) => void;
    onclose: () => void;
  } = $props();
  // Dialog-local draft. Cancel never alters current timing or persisted defaults.
  let draft = $state<Preferences>(
    untrack(() => ({ ...preferences, timing: { ...timing } })),
  );
  let dialog: HTMLDialogElement;
  let busy = $state(false);
  let error = $state('');
  let notice = $state('');
  onMount(() => dialog.showModal());
  async function save() {
    if (busy) return;
    busy = true;
    error = '';
    try {
      await savePreferences(draft);
      onapply(draft);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  async function preset(importing: boolean) {
    if (busy) return;
    busy = true;
    error = '';
    notice = '';
    try {
      const path = await chooseFile(
        importing ? 'timingPreset' : 'saveTimingPreset',
        importing ? null : 'timing.vsfpreset',
      );
      if (!path) return;
      if (importing) {
        const result = await importTimingPreset(path);
        draft.timing = { ...result.preset.timing };
        notice = `Imported ${result.preset.name}${result.warnings.length ? ' · ' + result.warnings.join(' · ') : ''}. Apply to use these values.`;
      } else {
        await exportTimingPreset(path, {
          version: 1,
          name: 'Custom timing',
          timing: draft.timing,
        });
        notice = 'Timing preset exported';
      }
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="settings-title"
  oncancel={(e) => {
    e.preventDefault();
    if (!busy) onclose();
  }}
>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <h2 id="settings-title">Settings</h2>
    <div class="body">
      <section aria-labelledby="scenes-settings-title">
        <h3 id="scenes-settings-title">Scene changes</h3>
        <fieldset disabled={busy}>
          <label
            ><input type="checkbox" bind:checked={draft.autoScanScenes} /> Scan when
            opening a video or project</label
          >
          <p>
            Completed project results are reused. You can start or cancel a scan
            from Tools at any time.
          </p>
          <label
            >Full scan detail
            <select bind:value={draft.sceneDetail}>
              <option value="standard">Standard</option>
              <option value="reduced">Reduced</option>
            </select>
          </label>
          <p>
            Reduced detail uses smaller images and may miss subtle cuts. Changes
            apply to the next scan. Use View → Show Scene Changes to hide or
            show markers.
          </p>
        </fieldset>
      </section>
      <TimingSettingsPanel
        value={draft.timing}
        disabled={busy}
        onchange={(value) => (draft.timing = value)}
        onimport={() => void preset(true)}
        onexport={() => void preset(false)}
      />
      <p>
        Apply updates this workspace and saves defaults for future videos. A
        project's saved timing settings take precedence when it opens.
      </p>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      {#if notice}<p role="status">{notice}</p>{/if}
    </div>
    <footer>
      <button type="button" disabled={busy} onclick={onclose}>Cancel</button>
      <button type="submit" disabled={busy}
        >{busy ? 'Saving…' : 'Apply settings'}</button
      >
    </footer>
  </form>
</dialog>

<style>
  dialog {
    width: min(620px, calc(100vw - 32px));
    max-height: calc(100dvh - 48px);
    overflow: hidden;
    border: 1px solid #49616f;
    border-radius: 10px;
    padding: 24px;
    color: #e7eef3;
    background: #192e3c;
  }
  dialog::backdrop {
    background: #08121bb3;
  }
  form {
    display: flex;
    flex-direction: column;
    max-height: calc(100dvh - 100px);
  }
  .body {
    overflow: auto;
    min-height: 0;
  }
  h2 {
    flex-shrink: 0;
    margin: 0 0 20px;
    font-size: 22px;
  }
  h3 {
    margin: 0 0 12px;
    font-size: 15px;
  }
  section {
    padding: 16px;
    margin-bottom: 12px;
    background: #20323f;
  }
  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }
  label {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  p {
    font-size: 12px;
    color: #b5c7d3;
    line-height: 1.5;
  }
  footer {
    flex-shrink: 0;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 20px;
  }
  .error {
    color: #ffb0ae;
  }
</style>
