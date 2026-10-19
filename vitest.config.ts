import { defineConfig, configDefaults } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
export default defineConfig({plugins:[svelte()],test:{exclude:[...configDefaults.exclude,'.cache/**','cli/**'],setupFiles:['./tests/setup.ts']},resolve:{conditions:['browser']}});
