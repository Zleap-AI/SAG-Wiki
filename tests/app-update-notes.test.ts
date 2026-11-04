// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,unmount} from 'svelte';
import AppUpdates from '../src/components/AppUpdates.svelte';
import {defaultUpdateSettings,updateState,updateDownload} from '../src/lib/updates';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke,Channel:class{}}));
let app:ReturnType<typeof mount>;
beforeEach(()=>{
 updateDownload.set({key:'',version:'',stage:'idle',downloaded:0,total:null,error:''});
 updateState.set({key:'',checking:false,info:null,error:''});
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});


it('renders release notes as formatted Markdown and keeps unsafe HTML out',async()=>{
 const notes='## 0.2.6\n\n- **快捷搜索**：支持独立窗口。\n- 查看 [使用指南](https://example.com/guide)。\n\n`Ctrl+Shift+Space`\n\n<script>alert("release notes")</script>';
 invoke.mockResolvedValueOnce({current_version:'0.2.5',latest_version:'0.2.6',available:true,notes,download_url:null});
 app=mount(AppUpdates,{target:document.body,props:{config:{...defaultUpdateSettings}}});
 document.querySelector<HTMLButtonElement>('button')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.update-notes')).not.toBeNull());
 const notesElement=document.querySelector('.update-notes')!;
 await vi.waitFor(()=>expect(notesElement.querySelector('h2')?.textContent,notesElement.innerHTML).toBe('0.2.6'));
 expect(notesElement.querySelector('strong')?.textContent).toBe('快捷搜索');
 expect(notesElement.querySelectorAll('ul > li')).toHaveLength(2);
 expect(notesElement.querySelector('a')?.getAttribute('href')).toBe('https://example.com/guide');
 expect(notesElement.querySelector('code')?.textContent).toBe('Ctrl+Shift+Space');
 expect(notesElement.textContent).not.toContain('## 0.2.6');
 expect(notesElement.textContent).not.toContain('**快捷搜索**');
 expect(notesElement.querySelector('script')).toBeNull();
});
