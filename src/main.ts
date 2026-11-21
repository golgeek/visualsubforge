import { mount } from 'svelte';
import App from './App.svelte';
import { getSystemRegion } from './lib/client';
import { setDateRegion } from './lib/ui/date-format';

const target = document.getElementById('app');
if (!target) throw new Error('Application mount element is missing');

void getSystemRegion()
  .catch(() => null)
  .then((region) => {
    setDateRegion(region);
    mount(App, { target });
  });
