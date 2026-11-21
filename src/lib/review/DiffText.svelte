<script lang="ts">
  import type { ReviewChange } from '../client';
  import { diffParts, type Side } from './presentation';
  let {
    words,
    side,
    singleLine = false,
  }: {
    words: ReviewChange['words'];
    side: Side;
    singleLine?: boolean;
  } = $props();
  const parts = $derived(diffParts(words, side, singleLine));
</script>

<span class="diff" class:single={singleLine}
  >{#each parts as part, i (i)}{#if part.kind === 'removed'}<del
        >{part.text}</del
      >{:else if part.kind === 'added'}<ins>{part.text}</ins
      >{:else}{part.text}{/if}{/each}</span
>

<style>
  .diff {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .single {
    white-space: pre;
  }
  del,
  ins {
    border-radius: 3px;
    padding-inline: 1px;
  }
  del {
    background: var(--diff-del-bg);
    color: var(--diff-del-text);
    text-decoration: line-through;
    text-decoration-color: var(--diff-del-line);
    text-decoration-thickness: 1.5px;
  }
  ins {
    background: var(--diff-ins-bg);
    color: var(--diff-ins-text);
    text-decoration: none;
    font-weight: 600;
  }
</style>
