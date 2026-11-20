<script lang="ts">
  import type { UpdateSession } from './update-session.svelte';
  let {
    session,
    consent,
    disabled,
  }: { session: UpdateSession; consent: boolean | null; disabled: boolean } =
    $props();
  const reasons = {
    connection: 'GitHub could not be reached.',
    timeout: 'The request timed out.',
    rateLimit: 'GitHub’s request limit was reached.',
    unavailable: 'GitHub is unavailable.',
    tooLarge: 'The release list was too large.',
    invalid: 'GitHub returned an unreadable release list.',
    storage: 'The update settings could not be read or saved.',
  };
</script>

{#if session.result || session.checking || (consent == null && !session.postponed)}
  <section class="updates" aria-label="Version updates">
    {#if session.checking}
      <span role="status">Checking for updates…</span>
    {:else if session.result?.kind === 'available'}
      <span role="status"
        >VisualSubForge {session.result.version} is available.</span
      >
      <button {disabled} onclick={() => void session.view()}
        >View release</button
      >
      <button {disabled} onclick={() => void session.dismiss()}>Not now</button>
    {:else if session.result?.kind === 'current' || session.result?.kind === 'failed'}
      <span role="status"
        >{session.result.kind === 'current'
          ? 'VisualSubForge is up to date.'
          : "Couldn't check for updates. " +
            reasons[session.result.reason]}</span
      >
      <button {disabled} onclick={() => void session.dismiss()}>Dismiss</button>
    {:else}
      <span
        ><strong>Check for new versions automatically?</strong> VSF contacts GitHub,
        which sees your IP address. No update is downloaded.</span
      >
      <button
        disabled={disabled || session.saving}
        onclick={() => void session.consent(true)}>Check automatically</button
      >
      <button
        disabled={disabled || session.saving}
        onclick={() => void session.consent(false)}>Don't check</button
      >
      <button
        disabled={disabled || session.saving}
        onclick={() => (session.postponed = true)}>Ask next launch</button
      >
    {/if}
    {#if session.error}<span role="alert">{session.error}</span>{/if}
  </section>
{/if}

<style>
  .updates {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem;
    padding: 0.3rem 0.6rem;
    background: var(--surface, Canvas);
    font-size: 0.85rem;
  }
  .updates span:first-child {
    flex: 1 1 16rem;
  }
</style>
