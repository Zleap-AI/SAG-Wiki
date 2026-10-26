import {defineConfig} from 'vite';
import {svelte} from '@sveltejs/vite-plugin-svelte';
export default defineConfig({plugins:[svelte()],optimizeDeps:{entries:['bench/docs.html']},server:{host:'127.0.0.1',port:1437,strictPort:true},build:{target:'es2022',outDir:'dist-docs',rollupOptions:{input:'bench/docs.html'}},preview:{host:'127.0.0.1',port:1437,strictPort:true}});
