import { mount } from 'svelte';
import App from '../../../packages/wallet/src/App.svelte';

const target = document.getElementById('app');
if (!target) {
  throw new Error('Could not find app element');
}

const app = mount(App, {
  target: target
});

export default app;
