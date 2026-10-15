import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
export default defineConfig({plugins:[svelte()],clearScreen:false,server:{strictPort:true,port:1420},envPrefix:['VITE_','TAURI_ENV_*'],build:{target:'es2022',sourcemap:false}});
