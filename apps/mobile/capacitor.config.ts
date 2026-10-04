import type { CapacitorConfig } from '@capacitor/cli';

const config: CapacitorConfig = {
  appId: 'io.swps.nockster',
  appName: 'Nockster',
  loggingBehavior: 'none',
  webDir: 'dist',
  ios: { contentInset: 'never', backgroundColor: '#ffffff' },
  android: { backgroundColor: '#ffffff', allowMixedContent: false },
  plugins: {
    // Native views own system-bar and keyboard insets. The headless WebView
    // must not add a second keyboard-sized padding to the Android window.
    SystemBars: { insetsHandling: 'disable' },
    CapacitorHttp: { enabled: true }
  }
};

export default config;
