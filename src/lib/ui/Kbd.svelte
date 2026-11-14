<script lang="ts">
  import { shortcutLabel } from '../workspace/action-registry';
  let {
    shortcut,
    mac = navigator.platform.startsWith('Mac'),
  }: { shortcut: string; mac?: boolean } = $props();
</script>

{#if shortcut}<kbd>{shortcutLabel(shortcut, mac)}</kbd>{/if}

<style>
  kbd {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
  }
</style>
