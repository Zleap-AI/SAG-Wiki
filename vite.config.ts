import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
// Lazily imported modules (answer renderer, editor, Mermaid, explore graph) pull in these
// packages after startup. Pre-bundling them keeps the dev server from re-optimizing and
// force-reloading the window in the middle of a conversation.
const lazyDependencies=['@tiptap/core','@tiptap/markdown','@tiptap/starter-kit','@tiptap/extension-image','@tiptap/extension-placeholder','@tiptap/extension-code-block','@tiptap/extension-list','@tiptap/extension-table','@tiptap/pm/model','@tiptap/pm/state','@tiptap/pm/view','dompurify','pele','graphology','graphology-layout-forceatlas2/worker','sigma'];
// KaTeX declares woff2, woff and ttf for every face. Every WebView SAG Wiki
// supports reads woff2, so dropping the legacy copies saves ~900 KB per build.
const katexWoff2Only={name:'katex-woff2-only',enforce:'pre' as const,transform(code:string,id:string){
 if(!/katex(\.min)?\.css$/.test(id))return;
 return code.replace(/src:url\(([^)]*\.woff2)\) format\("woff2"\),url\([^)]*\.woff\) format\("woff"\),url\([^)]*\.ttf\) format\("truetype"\)/g,'src:url($1) format("woff2")');
}};
export default defineConfig({plugins:[katexWoff2Only,svelte()],clearScreen:false,server:{strictPort:true,port:1420,watch:{ignored:['**/.cache/**','**/docs-site/**','**/browser-extension/build/**']}},envPrefix:['VITE_','TAURI_ENV_*'],optimizeDeps:{entries:['index.html'],include:lazyDependencies},build:{target:'es2022',sourcemap:false}});
