import { mount } from 'svelte';
import Harness from './Harness.svelte';
import Manual from './Manual.svelte';
const target = document.getElementById('test');
if (!target) throw new Error('Missing test root');
mount(new URLSearchParams(location.search).has('manual') ? Manual : Harness, {
  target,
});
