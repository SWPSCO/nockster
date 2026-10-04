import { defineConfig } from 'vite';
import { resolve } from 'node:path';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  root: import.meta.dirname,
  base: './',
  publicDir: '../../packages/wallet/public',
  plugins: [svelte()],
  resolve: { alias: { '@wallet': resolve(import.meta.dirname, '../../packages/wallet/src'), $lib: resolve(import.meta.dirname, '../../packages/wallet/src/lib') } },
  server: { watch: { ignored: ['**/target/**', '**/nockster-esp/**'] }, host: '127.0.0.1', port: 5175, strictPort: true, fs: { allow: [resolve(import.meta.dirname, '../..')] } },
  build: { outDir: 'dist', emptyOutDir: true, target: 'es2022' },
  clearScreen: false
});
