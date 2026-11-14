import { mount } from 'svelte';
import Harness from './Harness.svelte';
import Reference from './Reference.svelte';
const target = document.getElementById('test');
if (!target) throw new Error('Missing test root');
const query = new URLSearchParams(location.search);
if (query.has('preview')) {
  document.documentElement.dataset.theme =
    query.get('theme') === 'light' ? 'light' : 'dark';
  mount(Reference, { target });
} else mount(Harness, { target });
