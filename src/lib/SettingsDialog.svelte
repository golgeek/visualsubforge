<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import TimingSettingsPanel from './TimingSettings.svelte';
  import QualitySettings from './QualitySettings.svelte';
  import { copyProfile, profileTiming, withTiming } from './quality-profile';
  import {
    chooseFile,
    importTimingPreset,
    exportTimingPreset,
    savePreferences,
    getQualitySettings,
    importQualityProfile,
    exportQualityProfile,
    type QualityProfile,
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
    untrack(() => ({
      ...preferences,
      timing: { ...timing },
      quality: copyProfile(preferences.quality),
    })),
  );
  let dialog: HTMLDialogElement;
  let busy = $state(false);
  let error = $state('');
  let notice = $state('');
  let ready = $state(false);
  let epoch: number | null = null;
  let builtins = $state<QualityProfile[]>([]);
  let alive = true;
  onMount(() => {
    dialog.showModal();
    void getQualitySettings()
      .then((data) => {
        if (!alive) return;
        draft.quality = copyProfile(data.current.profile);
        draft.timing = { ...data.current.timing };
        builtins = data.builtins;
        epoch = data.current.epoch;
        ready = true;
      })
      .catch((e) => {
        if (alive) error = String(e);
      });
    return () => {
      alive = false;
    };
  });
  function quality(profile: QualityProfile) {
    draft.quality = profile;
    draft.timing = profileTiming(profile, draft.timing);
  }
  function updateTiming(timing: TimingSettings) {
    draft.quality = withTiming(draft.quality, timing);
    draft.timing = timing;
  }
  async function save() {
    if (busy || !ready || epoch === null) return;
    busy = true;
    error = '';
    try {
      const accepted = $state.snapshot(draft);
      await savePreferences(accepted, epoch);
      if (alive) onapply(accepted);
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) busy = false;
    }
  }
  async function preset(importing: boolean) {
    if (busy || !ready) return;
    busy = true;
    error = '';
    notice = '';
    try {
      const path = await chooseFile(
        importing ? 'timingPreset' : 'saveTimingPreset',
        importing ? null : 'timing.vsfpreset',
      );
      if (!path || !alive) return;
      if (importing) {
        const result = await importTimingPreset(path);
        if (!alive) return;
        updateTiming({ ...result.preset.timing });
        notice = `Imported ${result.preset.name}${result.warnings.length ? ' · ' + result.warnings.join(' · ') : ''}. Apply to use these values.`;
      } else {
        await exportTimingPreset(path, {
          version: 1,
          name: 'Custom timing',
          timing: draft.timing,
        });
        if (alive) notice = 'Timing preset exported';
      }
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) busy = false;
    }
  }
  async function qualityPreset(importing: boolean) {
    if (busy || !ready) return;
    busy = true;
    error = '';
    notice = '';
    try {
      const path = await chooseFile(
        importing ? 'qualityPreset' : 'saveQualityPreset',
        importing ? null : `${draft.quality.id}.vsfpreset`,
      );
      if (!path || !alive) return;
      if (importing) {
        const profile = await importQualityProfile(path);
        if (!alive) return;
        quality(copyProfile(profile));
        notice = `Imported ${profile.name}. Review its rules and coverage, then Apply to use it.`;
      } else {
        await exportQualityProfile(path, $state.snapshot(draft.quality));
        if (alive) notice = 'Quality profile exported';
      }
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) busy = false;
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
      {#if !ready && !error}<p role="status">
          Loading current workspace rules…
        </p>{/if}
      <section aria-labelledby="scenes-settings-title">
        <h3 id="scenes-settings-title">Scene changes</h3>
        <fieldset disabled={busy || !ready}>
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
      {#if ready}
        <label
          ><input
            type="checkbox"
            bind:checked={draft.showQualityMetrics}
            disabled={busy}
          /> Show cue metrics by default</label
        >
        <QualitySettings
          value={draft.quality}
          {builtins}
          disabled={busy}
          onchange={quality}
          onimport={() => void qualityPreset(true)}
          onexport={() => void qualityPreset(false)}
        />
      {/if}
      <TimingSettingsPanel
        value={draft.timing}
        disabled={busy || !ready}
        onchange={updateTiming}
        onimport={() => void preset(true)}
        onexport={() => void preset(false)}
      />
      <p>
        Apply updates this workspace and saves defaults for future videos. A
        project's saved quality profile and timing settings take precedence when
        it opens.
      </p>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      {#if notice}<p role="status">{notice}</p>{/if}
    </div>
    <footer>
      <button type="button" disabled={busy} onclick={onclose}>Cancel</button>
      <button type="submit" disabled={busy || !ready}
        >{busy ? 'Saving…' : 'Apply settings'}</button
      >
    </footer>
  </form>
</dialog>

<style>
  dialog {
    width: min(760px, calc(100vw - 32px));
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
