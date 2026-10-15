import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
export default defineConfig({plugins:[svelte()],test:{setupFiles:['./tests/setup.ts']},resolve:{conditions:['browser']}});
