<script lang="ts">
  import type { QualityRule } from '../client';
  import type { CueFixes } from './cue-fixes.svelte';
  import Chip from '../ui/Chip.svelte';
  import type { QualitySession } from './quality-session.svelte';
  import { qualityRules, qualityText } from './quality-display';
  let {
    quality,
    fixes,
    cueId,
    disabled,
    onmore,
    onfix,
    onreview,
  }: {
    quality: QualitySession;
    fixes: CueFixes;
    onfix: () => void;
    onreview: (rule: QualityRule) => void;
    cueId: number | null;
    disabled: boolean;
    onmore: () => void;
  } = $props();
  const projection = $derived(
    !quality.stale && quality.view?.selectedIssues?.cueId === String(cueId)
      ? quality.view.selectedIssues
      : null,
  );
  let selected = $state(0);
  let identity = '';
  $effect(() => {
    const next = JSON.stringify([quality.view?.key, cueId]);
    if (next !== identity) {
      identity = next;
      selected = 0;
    }
  });
  const issues = $derived(
    projection?.offset === 0 ? projection.issues.slice(0, 3) : [],
  );
  const current = $derived(issues[selected]);
  function label(rule: string) {
    return rule === 'rs_high'
      ? 'Too fast'
      : rule === 'rs_low'
        ? 'Too slow'
        : rule === 'line_length'
          ? 'Line too long'
          : rule === 'scene_start' || rule === 'scene_end'
            ? 'Near shot change'
            : (qualityRules.find((item) => item.id === rule)?.label ?? rule);
  }
</script>

<div class="issue-strip" aria-label="Selected cue issues">
  <div class="chips">
    {#each issues as issue, index (index)}
      <span
        class:error={issue.findings.some(
          (finding) => finding.severity === 'violation',
        )}
      >
        <Chip
          active={selected === index}
          {disabled}
          onclick={() => (selected = index)}>{label(issue.rule)}</Chip
        >
      </span>
    {/each}
    {#if projection && projection.count > 3}
      <button
        {disabled}
        onclick={onmore}
        aria-label={`Show all ${projection.count} cue issues`}
        >+{projection.count - 3}</button
      >
    {:else if !issues.length}
      <span
        >{cueId === null
          ? 'Select a cue'
          : quality.error
            ? 'Issues unavailable'
            : projection
              ? 'No issues'
              : quality.paused
                ? 'Issues paused'
                : 'Checking issues…'}</span
      >
    {/if}
    <span class="spacer"></span>
    {#if fixes.count}<button
        disabled={disabled || fixes.accepting}
        onclick={onfix}
        title="Apply safe fixes as one Undo step; never split a cue"
        >Fix {fixes.count}</button
      >
    {:else if fixes.error}<button
        {disabled}
        onclick={() => fixes.retry()}
        title={fixes.error}>Retry fixes</button
      >
    {:else if fixes.preparing}<span>Checking fixes…</span>
    {:else if projection?.count}<span
        title="Some issues need manual editing, a spelling choice, or current scene data"
        >Review manually</span
      >{/if}
  </div>
  <div
    class="detail"
    title={current?.findings
      .map((f) => qualityText(f.explanation))
      .join(' · ') ?? ''}
  >
    <span
      >{current?.findings.map((f) => qualityText(f.explanation)).join(' · ') ??
        '\u00a0'}</span
    >
    {#if current}<button {disabled} onclick={() => onreview(current!.rule)}
        >{current.rule === 'scene_start' || current.rule === 'scene_end'
          ? 'Snap to cut…'
          : 'Review fix…'}</button
      >{/if}
  </div>
</div>

<style>
  .spacer {
    flex: 1;
  }
  .issue-strip {
    height: 44px;
    min-height: 44px;
    font-size: 11px;
    overflow: hidden;
  }
  .chips {
    display: flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    white-space: nowrap;
  }
  .detail {
    display: flex;
    gap: 8px;
    align-items: center;
    height: 20px;
    line-height: 20px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
  }
  .detail span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .detail button {
    height: 18px;
    min-height: 0;
    padding: 0 6px;
    font-size: 10px;
    flex-shrink: 0;
  }
  .error :global(button) {
    border-color: var(--err);
  }
  button {
    padding: 1px 8px;
    border-radius: 12px;
  }
</style>
