import release from '../../release-version.json';
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { resolve } from 'node:path';
import { renameSync, rmSync } from 'node:fs';

export default defineConfig({
  root: import.meta.dirname,
  base: './',
  publicDir: '../../packages/wallet/public',
  plugins: [
    svelte(),
    {
      name: 'urbit-runtime-assets',
      generateBundle(_options, bundle) {
        for (const entry of Object.values(bundle)) {
          if (entry.type === 'asset' && entry.fileName.endsWith('.css')) {
            entry.source = String(entry.source).replaceAll(
              'Nokora-ExtraBold.ttf',
              'nokora-extrabold.ttf'
            );
          }
        }
      },
      closeBundle() {
        rmSync(resolve(import.meta.dirname, 'dist/vanity/miner.d.ts'), { force: true });
        // Vere skips filenames containing uppercase letters during Clay sync.
        for (const name of ['Nokora-ExtraBold.ttf', 'OFL-Nokora.txt']) {
          renameSync(
            resolve(import.meta.dirname, 'dist/fonts', name),
            resolve(import.meta.dirname, 'dist/fonts', name.toLowerCase())
          );
        }
      }
    }
  ],
  define: {
    'import.meta.env.VITE_APP_VERSION': JSON.stringify(
      process.env.NOCKSTER_VERSION || release.version
    )
  },
  resolve: { alias: { $lib: resolve(import.meta.dirname, '../../packages/wallet/src/lib') } },
  server: {
    host: '127.0.0.1',
    port: 5186,
    strictPort: true,
    watch: { ignored: ['**/target/**', '**/.zig-cache/**', '**/zig-out/**'] },
    fs: { allow: [resolve(import.meta.dirname, '../..')] }
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
    rollupOptions: { output: { hashCharacters: 'hex', chunkFileNames: 'assets/chunk-[hash].js' } }
  }
});
