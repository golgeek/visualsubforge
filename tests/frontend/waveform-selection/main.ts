import { mount } from 'svelte';
import Harness from './Harness.svelte';
const target = document.getElementById('test');
if (!target) throw new Error('Missing test root');
mount(Harness, { target });
