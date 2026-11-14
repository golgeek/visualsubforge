<script lang="ts">
  import type { RecentWork } from './recent-work.svelte';
  import type { RecentEntry } from '../client';
  let {
    recent,
    disabled,
    onopen,
  }: {
    recent: RecentWork;
    disabled: boolean;
    onopen: (entry: RecentEntry) => Promise<void>;
  } = $props();
  const name = (path: string) => path.split(/[\\/]/).pop() || path;
</script>

<section class="recent" aria-label="Recent work">
  <h2>Recent work</h2>
  {#if recent.error}<p role="alert">{recent.error}</p>
    <button disabled={recent.loading} onclick={() => void recent.load()}
      >Reload recent work</button
    >{/if}
  {#if recent.loading}<p>Loading recent work…</p>{/if}
  {#if !recent.entries.length && !recent.loading}<p>
      Opened work will appear here.
    </p>{/if}
  <ul>
    {#each recent.entries as entry (`${entry.kind}:${entry.path}`)}
      <li>
        <button
          class="open"
          disabled={disabled || recent.removing}
          title={entry.path}
          onclick={() => void onopen(entry)}
        >
          <strong>{name(entry.path)}</strong>
          <span
            >{entry.kind === 'project'
              ? 'VSF project'
              : entry.kind === 'media'
                ? 'Video / audio'
                : 'Subtitles'}{entry.mediaPath
              ? ` · ${name(entry.mediaPath)}`
              : ''}</span
          >
          <small>Last used {new Date(entry.usedAtMs).toLocaleString()}</small>
          {#if entry.summary}
            <span
              >{entry.summary.cues} cues · {entry.summary.issues} issues</span
            >
            <small
              title={`Source SHA-256: ${entry.summary.sourceSha256}\nProfile: ${entry.summary.profileHash}`}
            >
              Last checked {new Date(
                entry.summary.checkedAtMs,
              ).toLocaleString()} · {entry.summary.profileName}{entry.summary
                .complete
                ? ''
                : ' · partial coverage'}
            </small>
          {:else}<small>Issue count unavailable</small>{/if}
        </button>
        <button
          class="forget"
          aria-label={`Remove ${name(entry.path)} from recent work`}
          disabled={disabled || recent.removing}
          onclick={() => void recent.forget(entry)}>Remove</button
        >
      </li>
    {/each}
  </ul>
  {#if recent.entries.length}<p class="hint">
      Counts describe the last recorded saved file, not a fresh check. Remove
      only clears this list entry.
    </p>{/if}
</section>

<style>
  .recent {
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    padding: 16px;
    background: var(--surface-panel);
    max-height: 380px;
    overflow: auto;
  }
  h2 {
    margin: 0 0 12px;
    font-size: 16px;
  }
  p,
  small {
    color: var(--text-muted);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    gap: 8px;
    align-items: center;
    border-top: 1px solid var(--line);
    padding: 8px 0;
  }
  .open {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 4px;
    align-items: start;
    text-align: left;
    white-space: normal;
    background: transparent;
    border-color: transparent;
  }
  .open strong,
  .open span,
  small {
    overflow-wrap: anywhere;
  }
  .open span {
    color: var(--text-muted);
  }
  .forget {
    font-size: var(--font-size-xs);
  }
  .hint {
    font-size: var(--font-size-xs);
    margin-bottom: 0;
    line-height: 1.5;
  }
</style>
