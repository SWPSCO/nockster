import '../../../packages/wallet/src/app.css';
import '../../../packages/wallet/src/styles/global.css';
import '../../../packages/wallet/src/styles/variables.css';
import '../../../packages/wallet/src/styles/themes.css';
import { mount } from 'svelte';
import App from '../../../packages/wallet/src/App.svelte';
import { priceStore } from '../../../packages/wallet/src/lib/stores/price';

console.log('Svelte mounting...');

// Initialize price store
priceStore.init().catch(console.error);

const target = document.getElementById('app');
if (!target) {
  console.error('Could not find app element');
  throw new Error('Could not find app element');
}

console.log('Target element found, mounting app...');

let app;
try {
  app = mount(App, {
    target: target
  });
  console.log('App mounted successfully');
} catch (error) {
  console.error('Failed to mount app:', error);
  throw error;
}

export default app;
