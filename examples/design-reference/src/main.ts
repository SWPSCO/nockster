import './lib/styles/global.css'
import { mount } from 'svelte'

// Check if we're in demo mode
const isDemoMode = window.location.pathname === '/demo';

let AppComponent;
if (isDemoMode) {
  // Dynamically import Demo component for demo route
  AppComponent = (await import('./Demo.svelte')).default;
} else {
  // Toggle between these two imports to switch views:
  // AppComponent = (await import('./App.svelte')).default; // Single screen view
  AppComponent = (await import('./Showcase.svelte')).default; // Showcase view with all screens
}

const app = mount(AppComponent, {
  target: document.getElementById('app')!,
})

export default app