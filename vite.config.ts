import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
// Lazily imported modules (answer renderer, editor, Mermaid, explore graph) pull in these
// packages after startup. Pre-bundling them keeps the dev server from re-optimizing and
// force-reloading the window in the middle of a conversation.
const lazyDependencies=['@tiptap/core','@tiptap/markdown','@tiptap/starter-kit','@tiptap/extension-image','@tiptap/extension-placeholder','@tiptap/extension-code-block','@tiptap/extension-list','@tiptap/extension-table','@tiptap/pm/model','@tiptap/pm/state','@tiptap/pm/view','dompurify','pele','graphology','graphology-layout-forceatlas2/worker','sigma'];
export default defineConfig({plugins:[svelte()],clearScreen:false,server:{strictPort:true,port:1420},envPrefix:['VITE_','TAURI_ENV_*'],optimizeDeps:{entries:['index.html'],include:lazyDependencies},build:{target:'es2022',sourcemap:false}});
