import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { resolve } from 'path';
import { copyFileSync, existsSync, mkdirSync } from 'fs';

export default defineConfig(({ mode }) => {
  const isDev = mode === 'development';

  return {
    root: import.meta.dirname,
    envDir: resolve(import.meta.dirname, '../..'),
    publicDir: '../../packages/wallet/public',
    plugins: [
      svelte(),
      {
        name: 'copy-assets',
        writeBundle() {
          // Copy icons from ext/icons to ext/dist/icons
          const iconsDir = resolve(__dirname, 'ext/dist/icons');
          if (!existsSync(iconsDir)) {
            mkdirSync(iconsDir, { recursive: true });
          }
          const icons = ['icon-16.png', 'icon-48.png', 'icon-128.png'];
          icons.forEach(icon => {
            const src = resolve(__dirname, 'ext/icons', icon);
            const dest = resolve(__dirname, 'ext/dist/icons', icon);
            if (existsSync(src)) {
              copyFileSync(src, dest);
              console.log(`Copied ${icon} to dist/icons`);
            }
          });

          // Copy WASM file from src/pkg to dist
          const wasmSrc = resolve(__dirname, '../../packages/wallet/src/pkg', 'nockster_core_bg.wasm');
          const wasmDest = resolve(__dirname, 'ext/dist', 'nockster_core_bg.wasm');
          if (existsSync(wasmSrc)) {
            copyFileSync(wasmSrc, wasmDest);
            console.log('Copied nockster_core_bg.wasm to dist');
          } else {
            console.warn(`WASM file not found at ${wasmSrc}`);
          }
        }
      }
    ],
    base: isDev ? '/' : './',

    server: {
      fs: { allow: [resolve(import.meta.dirname, '../..')] },
      watch: { ignored: ['**/target/**'] },
      ...(isDev
        ? {
            port: 5173,
            open: false,
            hmr: {
              host: 'localhost',
              port: 5173,
              protocol: 'ws'
            }
          }
        : {})
    },

    build: {
      // Extension service workers have no document for Vite's preload helper.
      modulePreload: false,
      outDir: 'ext/dist',
      emptyOutDir: true,
      watch: isDev
        ? {
            include: 'src/**'
          }
        : undefined,
      rollupOptions: {
        input: {
          popup: resolve(__dirname, 'index.html'),
          approval: resolve(__dirname, 'approval.html'),
          inpage: resolve(__dirname, 'src/inpage.ts'),
          background: resolve(__dirname, 'src/background.ts'),
          contentScript: resolve(__dirname, 'src/contentScript.ts')
        },
        output: {
          entryFileNames: '[name].js',
          chunkFileNames: '[name].js',
          assetFileNames: '[name].[ext]'
        }
      }
    },

    resolve: {
      alias: {
        $lib: resolve(__dirname, '../../packages/wallet/src/lib')
      },
      extensions: ['.js', '.ts', '.jsx', '.tsx', '.json', '.svelte']
    },

    assetsInclude: ['**/*.wasm']
  };
});
