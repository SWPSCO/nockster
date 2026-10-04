import '../../../packages/wallet/src/app.css';
import '../../../packages/wallet/src/styles/variables.css';
import { mount } from 'svelte';
import WebsiteApproval from './WebsiteApproval.svelte';

mount(WebsiteApproval, { target: document.getElementById('app')! });
