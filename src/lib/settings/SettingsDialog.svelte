<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import TimingSettingsPanel from './TimingSettings.svelte';
  import QualitySettings from './QualitySettings.svelte';
  import ShortcutsSettings from './ShortcutsSettings.svelte';
  import { SettingsSession } from './settings-session.svelte';
  import {
    copyProfile,
    profileTiming,
    withTiming,
  } from '../quality/quality-profile';
  import { qualityRules } from '../quality/quality-display';
  import type { QualitySession } from '../quality/quality-session.svelte';
  import {
    chooseFile,
    importTimingPreset,
    exportTimingPreset,
    importQualityProfile,
    exportQualityProfile,
    type QualityProfile,
    type Preferences,
  } from '../client';
  import type { TimingSettings } from '../timing/magnetic-timing';
  let {
    preferences,
    timing,
    checks,
    onapply,
    onclose,
  }: {
    preferences: Preferences;
    timing: TimingSettings;
    checks?: QualitySession;
    onapply: (value: Preferences) => void;
    onclose: () => void;
  } = $props();
  let dialog: HTMLDialogElement;
  let form: HTMLFormElement;
  let importing = $state(false);
  let transferError = $state('');
  let transferNotice = $state('');
  let alive = true;
  const session = untrack(
    () =>
      new SettingsSession(
        { ...preferences, timing: { ...timing } },
        (value) => onapply(value),
        () => {
          if (importing) return 'Wait for the profile transfer to finish';
          const invalid = form?.querySelector<
            HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement
          >(':invalid');
          return invalid
            ? `${invalid.labels?.[0]?.textContent?.trim() ?? 'Setting'}: ${invalid.validationMessage}`
            : null;
        },
      ),
  );
  const draft = $derived(session.draft);
  const disabled = $derived(!session.ready || session.loading || importing);
  const counts = $derived(
    checks && !checks.stale && checks.view?.phase === 'ready'
      ? (checks.view.ruleCues ?? [])
      : null,
  );
  const pages = [
    {
      id: 'general',
      label: 'General',
      keywords: 'defaults workspace project preferences',
    },
    {
      id: 'editing',
      label: 'Editing',
      keywords: 'cue metrics auto commit text undo history',
    },
    {
      id: 'timing',
      label: 'Timing and snapping',
      keywords:
        'gap before after cut scene buffer snap distance import export preset',
    },
    {
      id: 'quality',
      label: 'Quality profile',
      keywords:
        'duration minimum maximum cps rs speed lines characters empty overlaps rules import export',
    },
    {
      id: 'language',
      label: 'Spelling and language',
      keywords:
        'dictionary personal words typography capitalization dialogue french english license',
    },
    {
      id: 'video',
      label: 'Video and shot changes',
      keywords: 'scan scene detail reduced standard media',
    },
    {
      id: 'shortcuts',
      label: 'Shortcuts',
      keywords: 'keyboard rebind conflict preset classic default keys',
    },
    {
      id: 'appearance',
      label: 'Appearance',
      keywords: 'layout density columns theme fonts',
    },
    {
      id: 'plugins',
      label: 'Plugins',
      keywords: 'javascript checker installed packages',
    },
    {
      id: 'advanced',
      label: 'Advanced',
      keywords:
        'counting units utf16 source rendered speed calculation coverage',
    },
  ] as const;
  type Page = (typeof pages)[number]['id'];
  let page = $state<Page>('general');
  let query = $state('');
  const matching = $derived(
    pages.filter((item) =>
      `${item.label} ${item.keywords}`
        .toLocaleLowerCase()
        .includes(query.trim().toLocaleLowerCase()),
    ),
  );
  $effect(() => {
    if (matching.length && !matching.some((item) => item.id === page))
      page = matching[0]!.id;
  });
  const shown = (id: Page) => page === id && matching.length > 0;
  const wordLanguages = [
    ['englishUs', 'English (US) personal words'],
    ['french', 'French personal words'],
  ] as const;
  onMount(() => {
    dialog.showModal();
    void session.load();
    return () => {
      alive = false;
      session.dispose();
    };
  });
  function profile(value: QualityProfile) {
    draft.quality = value;
    draft.timing = profileTiming(value, draft.timing);
    void session.changed();
  }
  function updateTiming(value: TimingSettings) {
    draft.quality = withTiming(draft.quality, value);
    draft.timing = value;
    void session.changed();
  }
  async function close() {
    if (session.saving || importing) return;
    if (!session.ready || (await session.flush())) onclose();
  }
  async function transfer(kind: 'timing' | 'quality', importingFile: boolean) {
    if (disabled || session.saving || session.uncertain) return;
    importing = true;
    transferError = '';
    transferNotice = '';
    try {
      const path = await chooseFile(
        importingFile
          ? kind === 'timing'
            ? 'timingPreset'
            : 'qualityPreset'
          : kind === 'timing'
            ? 'saveTimingPreset'
            : 'saveQualityPreset',
        importingFile
          ? null
          : kind === 'timing'
            ? 'timing.vsfpreset'
            : `${draft.quality.id}.vsfpreset`,
      );
      if (!path || !alive) return;
      if (importingFile) {
        if (kind === 'timing') {
          const result = await importTimingPreset(path);
          if (!alive) return;
          updateTiming({ ...result.preset.timing });
          transferNotice = `Imported ${result.preset.name}${result.warnings.length ? ' · ' + result.warnings.join(' · ') : ''}.`;
        } else {
          const result = await importQualityProfile(path);
          if (!alive) return;
          profile(copyProfile(result));
          transferNotice = `Imported ${result.name}.`;
        }
      } else if (kind === 'timing') {
        await exportTimingPreset(path, {
          version: 1,
          name: 'Custom timing',
          timing: draft.timing,
        });
        if (alive) transferNotice = 'Timing preset exported';
      } else {
        await exportQualityProfile(path, $state.snapshot(draft.quality));
        if (alive) transferNotice = 'Quality profile exported';
      }
    } catch (error) {
      if (alive) transferError = String(error);
    } finally {
      if (alive) {
        importing = false;
        if (importingFile) void session.changed();
      }
    }
  }
</script>

<dialog
  bind:this={dialog}
  aria-labelledby="settings-title"
  oncancel={(event) => {
    event.preventDefault();
    void close();
  }}
>
  <form
    bind:this={form}
    onsubmit={(event) => event.preventDefault()}
    oninput={() => void session.changed()}
    onchange={() => void session.changed()}
  >
    <header>
      <h2 id="settings-title">Settings</h2>
      <span>Changes save automatically</span>
    </header>
    <div class="body">
      <nav aria-label="Settings pages">
        <input
          type="search"
          aria-label="Search settings"
          placeholder="Search settings"
          bind:value={query}
        />
        {#each matching as item (item.id)}<button
            type="button"
            aria-current={page === item.id ? 'page' : undefined}
            onclick={() => (page = item.id)}>{item.label}</button
          >{/each}
        {#if !matching.length}<p>No matching settings.</p>{/if}
      </nav>
      <main class="settings-content">
        {#if session.loading}<p role="status">
            Loading current workspace settings…
          </p>{/if}
        <section hidden={!shown('general')}>
          <h3>General</h3>
          <p>
            Valid changes update this workspace and save your defaults. A
            project’s saved quality profile and timing settings take precedence
            when it opens. Settings changes never retime cues or enter subtitle
            Undo history.
          </p>
          <p>
            Use Undo below to restore the last confirmed settings change.
            Invalid fields remain unsaved until corrected.
          </p>
        </section>
        <section hidden={!shown('editing')}>
          <h3>Editing</h3>
          <label
            ><input
              type="checkbox"
              {disabled}
              bind:checked={draft.showQualityMetrics}
            /> Show cue metrics by default</label
          >
          <p>
            Cue edits save to the accepted document after a 700 ms pause, on
            blur, or before a command. Same-cue typing in one focus session
            shares Undo until five seconds idle. Escape restores the confirmed
            cue; saving the subtitle file remains explicit.
          </p>
        </section>
        <div hidden={!shown('timing')}>
          <TimingSettingsPanel
            value={draft.timing}
            {disabled}
            onchange={updateTiming}
            onimport={() => void transfer('timing', true)}
            onexport={() => void transfer('timing', false)}
          />
        </div>
        <div
          hidden={!(shown('quality') || shown('language') || shown('advanced'))}
        >
          <QualitySettings
            value={draft.quality}
            page={page === 'language'
              ? 'language'
              : page === 'advanced'
                ? 'advanced'
                : 'quality'}
            builtins={session.builtins}
            {disabled}
            onchange={profile}
            onimport={() => void transfer('quality', true)}
            onexport={() => void transfer('quality', false)}
          />
        </div>
        <section hidden={!shown('quality')} class="rule-counts">
          <h4>Cues flagged now</h4>
          <p>
            Counts describe the saved rules and accepted subtitle document,
            including related cues. Unsaved or invalid settings are not
            included.
          </p>
          {#if counts === null}<p role="status">
              {checks?.error || 'Waiting for an accepted quality report…'}
            </p>{:else}<dl>
              {#each qualityRules as rule (rule.id)}<div>
                  <dt>{rule.label}</dt>
                  <dd>
                    {counts.find((count) => count.rule === rule.id)?.cues ?? 0}
                  </dd>
                </div>{/each}
            </dl>{/if}
        </section>
        <section hidden={!shown('language')}>
          <h4>Personal spelling words</h4>
          <p>
            One word per line, up to 32 characters each and 2048 total. Saved
            for this user; session ignores remain separate.
          </p>
          {#each wordLanguages as [key, label] (key)}<label
              >{label}<textarea
                aria-label={label}
                rows="4"
                {disabled}
                value={draft.personalWords[key].join('\n')}
                oninput={(event) => {
                  draft.personalWords = {
                    ...draft.personalWords,
                    [key]: event.currentTarget.value
                      .split('\n')
                      .map((word) => word.trim())
                      .filter(Boolean),
                  };
                }}></textarea></label
            >{/each}
        </section>
        <section hidden={!shown('video')}>
          <h3>Video and shot changes</h3>
          <fieldset {disabled}>
            <label
              ><input type="checkbox" bind:checked={draft.autoScanScenes} /> Scan
              when opening video</label
            ><label
              >Scan detail <select
                aria-label="Scene scan detail"
                bind:value={draft.sceneDetail}
                ><option value="reduced">Reduced</option><option
                  value="standard">Standard</option
                ></select
              ></label
            >
          </fieldset>
          <p>
            Reduced detail uses smaller images and may miss subtle cuts. Changes
            affect the next scan; completed project results are reused. Existing
            cue timings stay unchanged.
          </p>
        </section>
        <div hidden={!shown('shortcuts')}>
          <ShortcutsSettings
            value={draft.shortcuts}
            {disabled}
            onchange={(value) => {
              draft.shortcuts = value;
              void session.changed();
            }}
          />
        </div>
        <section hidden={!shown('appearance')}>
          <h3>Appearance</h3>
          <p>
            Use Layout to choose Timing, Editor above list, Review, Translation
            or Big picture. Each preset remembers its pane sizes and cue
            columns.
          </p>
          <p>
            Columns → Row density selects compact or comfortable cue rows for
            the active preset. Font choices for subtitle rendering remain in
            Fonts.
          </p>
        </section>
        <section hidden={!shown('plugins')}>
          <h3>Plugins</h3>
          <p>
            Open Subtitle Plugins from Commands to choose an installed checker
            or review its proposed fixes. Each plugin declares its own supported
            settings and scope; fixes require explicit review.
          </p>
        </section>
        {#if !matching.length}<p>
            No matching settings. Try “duration”, “snapping” or “shortcuts”.
          </p>{/if}
      </main>
    </div>
    <footer>
      <div class="feedback" aria-live="polite">
        {#if session.validation}<p class="error">
            {session.validation} — not saved.
          </p>{/if}{#if session.error || transferError}<p
            class="error"
            role="alert"
          >
            {session.error || transferError}
          </p>{/if}
        <p>
          {session.saving
            ? 'Saving…'
            : session.notice ||
              transferNotice ||
              (session.dirty ? 'Unsaved changes' : 'Settings are up to date')}
        </p>
      </div>
      <div class="footer-actions">
        <button
          type="button"
          disabled={!session.canUndo || importing}
          onclick={() => void session.undo()}>Undo settings change</button
        >
        {#if session.uncertain}<button
            type="button"
            disabled={session.loading}
            onclick={() => void session.reconcile()}
            >Check saved settings</button
          >{:else if session.dirty && session.notice.includes('draft is retained')}<button
            type="button"
            disabled={disabled || session.saving}
            onclick={() => void session.flush()}>Retry save</button
          >{/if}
        {#if session.validation || session.error || session.uncertain}<button
            type="button"
            disabled={session.saving || importing}
            onclick={onclose}>Close without saving draft</button
          >{/if}
        <button
          type="button"
          disabled={session.saving || importing}
          onclick={() => void close()}>Close</button
        >
      </div>
    </footer>
  </form>
</dialog>

<style>
  dialog {
    width: min(960px, calc(100vw - 32px));
    height: min(720px, calc(100dvh - 40px));
    max-height: calc(100dvh - 40px);
    overflow: hidden;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    padding: 0;
    color: var(--text);
    background: var(--surface-panel);
  }
  dialog::backdrop {
    background: var(--scrim);
  }
  form {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--line);
    gap: 16px;
  }
  h2 {
    margin: 0;
    font-size: 20px;
  }
  header span {
    font-size: 12px;
    color: var(--text-muted);
  }
  .body {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  nav {
    flex: 0 0 190px;
    overflow: auto;
    padding: 12px;
    border-right: 1px solid var(--line);
  }
  nav input {
    width: 100%;
    min-width: 0;
    margin-bottom: 12px;
  }
  nav button {
    display: block;
    width: 100%;
    text-align: left;
    border-color: transparent;
    background: transparent;
    margin-bottom: 3px;
    font-size: 12px;
  }
  nav button[aria-current='page'] {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .settings-content {
    flex: 1;
    min-width: 0;
    overflow: auto;
    padding: 18px;
  }
  [hidden] {
    display: none !important;
  }
  section {
    padding: 16px;
    background: var(--surface-raised);
    margin-bottom: 12px;
  }
  h3,
  h4 {
    margin: 0 0 12px;
  }
  h3 {
    font-size: 16px;
  }
  h4 {
    font-size: 13px;
  }
  fieldset {
    border: 0;
    padding: 0;
    margin: 0;
  }
  label {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
    font-size: 12px;
  }
  label:has(textarea) {
    display: block;
  }
  textarea {
    display: block;
    width: 100%;
    margin-top: 6px;
  }
  p {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
  }
  footer {
    border-top: 1px solid var(--line);
    padding: 10px 16px;
    flex-shrink: 0;
  }
  .feedback {
    max-height: 72px;
    overflow: auto;
  }
  .feedback p {
    margin: 0 0 6px;
  }
  .error {
    color: var(--err);
  }
  .footer-actions {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 8px;
  }
  dl {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px 18px;
    font-size: 12px;
  }
  dl div {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  dd {
    color: var(--accent);
    font-variant-numeric: tabular-nums;
    margin: 0;
  }
  @media (max-width: 650px) {
    nav {
      flex-basis: 150px;
    }
    .settings-content {
      padding: 8px;
    }
    dl {
      grid-template-columns: 1fr;
    }
  }
</style>
