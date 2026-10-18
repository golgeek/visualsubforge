<script lang="ts">
  import { onDestroy } from 'svelte';
  import { getLanguageNotices, type LanguageNotices } from './client';
  let notices = $state.raw<LanguageNotices | null>(null);
  let error = $state('');
  let pending = $state(false);
  let alive = true;
  onDestroy(() => {
    alive = false;
  });
  async function load() {
    if (pending || notices) return;
    pending = true;
    try {
      const value = await getLanguageNotices();
      if (alive) notices = value;
    } catch (e) {
      if (alive) error = String(e);
    } finally {
      if (alive) pending = false;
    }
  }
</script>

<details>
  <summary>Dictionary sources, licenses and limitations</summary>
  <p>
    English (US): SCOWL 2020.12.07, size 60. French: Grammalecte Classique 7.7
    (MPL-2.0). Bundled immutable data from LibreOffice dictionaries. Spellbook
    0.4.2 (MPL-2.0) is an alpha Hunspell-compatible engine; some compound and
    phonetic behavior differs. Suggestions are limited to eight and exclude
    dictionary-wide ngram guesses.
  </p>
  <p>
    Words are normalized only for lookup. Names and dialect variants may need
    personal words. Unknown tags, drawings and karaoke require manual review.
    These are editorial aids, not a grammar or broadcaster compliance check.
  </p>
  <button type="button" disabled={pending} onclick={() => void load()}
    >Read bundled license notices</button
  >
  {#if error}<p role="alert">{error}</p>{/if}
  {#if notices}
    {#each [['English dictionary', notices.english], ['French dictionary', notices.french], ['Mozilla Public License 2.0', notices.mpl], ['Pinned sources and hashes', notices.sources]] as [title, text] (title)}
      <details>
        <summary>{title}</summary>
        <pre>{text}</pre>
      </details>
    {/each}
  {/if}
</details>

<style>
  pre {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 260px;
    overflow: auto;
    font-size: 11px;
  }
</style>
