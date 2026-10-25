// Production build of the benchmark page: npx vite build -c bench/vite.config.ts && npx vite preview -c bench/vite.config.ts
import {defineConfig} from 'vite';
import {svelte} from '@sveltejs/vite-plugin-svelte';
export default defineConfig({plugins:[svelte()],build:{target:'es2022',outDir:'dist-bench',emptyOutDir:true,rollupOptions:{input:'bench/preview.html'}},preview:{port:1431,strictPort:true}});
