import '../../../packages/wallet/src/app.css';
import '../../../packages/wallet/src/styles/global.css';
import '../../../packages/wallet/src/styles/variables.css';
import '../../../packages/wallet/src/styles/themes.css';
import './urbit.css';
import { mount } from 'svelte';
import UrbitApp from './UrbitApp.svelte';
import { priceStore } from '../../../packages/wallet/src/lib/stores/price';

void priceStore.init();
mount(UrbitApp, { target: document.getElementById('app')! });
