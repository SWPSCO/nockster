import release from '../../release-version.json';
import { defineConfig } from 'vite';
import { resolve } from 'node:path';

export default defineConfig({
  define: {
    'import.meta.env.VITE_APP_VERSION': JSON.stringify(process.env.NOCKSTER_VERSION || release.version)
  },
  root: import.meta.dirname,
  base: './',
  publicDir: '../../packages/wallet/public',
  resolve: {
    alias: {
      '/src': resolve(import.meta.dirname, '../../packages/wallet/src'),
      $lib: resolve(import.meta.dirname, '../../packages/wallet/src/lib')
    }
  },
  server: {
    watch: { ignored: ['**/target/**', '**/nockster-esp/**'] },
    host: '127.0.0.1',
    port: 5174,
    strictPort: true,
    fs: { allow: [resolve(import.meta.dirname, '../..')] }
  },
  build: { outDir: 'dist', emptyOutDir: true, target: 'es2022' },
  clearScreen: false
});
