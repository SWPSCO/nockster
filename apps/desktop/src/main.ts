import '../../../packages/wallet/src/app.css';
import '../../../packages/wallet/src/styles/global.css';
import '../../../packages/wallet/src/styles/variables.css';
import '../../../packages/wallet/src/styles/themes.css';
import './desktop.css';
import { mount } from 'svelte';
import { fetch as nativeFetch } from '@tauri-apps/plugin-http';
import { openUrl } from '@tauri-apps/plugin-opener';
import DesktopApp from './DesktopApp.svelte';
import { priceStore } from '../../../packages/wallet/src/lib/stores/price';
import { setSigningDeviceProvider } from '../../../packages/wallet/src/lib/utils/hardwareDevice';
import { hardwareSession } from './hardware/session';

setSigningDeviceProvider(hardwareSession.signingProvider);

const webFetch = globalThis.fetch.bind(globalThis);
globalThis.fetch = (input, init) => {
  const url = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
  const destination = new URL(url, location.href);
  return /^https?:/.test(destination.protocol) && destination.origin !== location.origin
    ? nativeFetch(input, { ...init, maxRedirections: 0 })
    : webFetch(input, init);
};

function openExternal(url: string) {
  void openUrl(url).catch(error => {
    window.dispatchEvent(
      new CustomEvent('desktop-error', { detail: `Unable to open link: ${String(error)}` })
    );
  });
}
window.open = url => {
  if (url) openExternal(String(url));
  return null;
};
document.addEventListener('click', event => {
  const link = (event.target as Element).closest<HTMLAnchorElement>('a[href]');
  if (link && /^https?:/.test(link.href)) {
    event.preventDefault();
    openExternal(link.href);
  }
});
if (!localStorage.getItem('theme')) localStorage.setItem('theme', 'dark');
void priceStore.init();
mount(DesktopApp, { target: document.getElementById('app')! });
