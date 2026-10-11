import { defineConfig } from 'vite';

export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // The pet skins live in ../pets (repo root), outside the Vite root (app/).
    fs: { allow: ['..'] },
  },
  build: {
    target: ['es2021', 'chrome100', 'safari13'],
    // No inline scripts: keeps a strict CSP (script-src 'self') possible.
    modulePreload: { polyfill: false },
    assetsInlineLimit: 0,
    sourcemap: false,
  },
});
