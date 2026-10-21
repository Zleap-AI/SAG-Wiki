import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
export default defineConfig({
  plugins: [svelte()], base: '/app/sag-desktop-fnos/', clearScreen: false,
  server: { host: '127.0.0.1', strictPort: true, port: 1421 },
  build: { target: 'es2022', sourcemap: false, rollupOptions: { input: { app: 'index.html', callback: 'auth-callback.html' } } },
});
