// @vitest-environment jsdom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import NoteImport from '../src/components/NoteImport.svelte';
import {setLanguage} from '../src/lib/i18n';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let app:ReturnType<typeof mount>|undefined;
const button=(name:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(item=>item.textContent===name)!;
async function message(data:object){const frame=document.querySelector('iframe')!;window.dispatchEvent(new MessageEvent('message',{source:frame.contentWindow,origin:location.origin,data:{channel:'sag-note-import',...data}}));await tick();}
beforeEach(()=>{
 window.matchMedia=vi.fn().mockReturnValue({matches:true,addEventListener(){},removeEventListener(){}});
 invoke.mockImplementation(async command=>{
 if(command==='prepare_note_import')return{session:123,destination:'/Documents/sag-wiki/notes/Notion',folders:[{ID:2,NAME:'工作'}]};
 if(command==='choose_note_import_folder')return '/Archive';
 if(command==='finish_note_import')return '/Documents/sag-wiki/notes/Notion';
});});
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';vi.clearAllMocks();setLanguage('zh-CN');});
async function start(source:'notion'|'evernote'|'keep'|'apple'){
 const done=vi.fn(async()=>{});app=mount(NoteImport,{target:document.body,props:{source,onClose:vi.fn(),onImported:done}});
 await vi.waitFor(()=>expect(document.body.textContent).toContain('/Documents/sag-wiki/notes'));
 await message({type:'ready'});return done;
}
it('文件导入需先选择文件，完成后刷新知识库',async()=>{
 const done=await start('notion');expect(button('导入').disabled).toBe(true);
 const input=document.querySelector<HTMLInputElement>('input[type=file]')!;
 Object.defineProperty(input,'files',{value:[new File(['zip'],'export.zip')]});input.dispatchEvent(new Event('change'));await tick();
 expect(button('导入').disabled).toBe(false);button('导入').click();await tick();expect(button('取消').disabled).toBe(true);
 await message({type:'complete',notes:4,attachments:5,log:[]});
 await vi.waitFor(()=>expect(done).toHaveBeenCalledOnce());expect(document.body.textContent).toContain('已导入 4 篇笔记');
});
it('数据库仅由 Apple Notes 入口读取，保留文件夹选择',async()=>{
 await start('apple');expect(document.querySelector('input[type=file]')).toBeNull();
 const checkbox=document.querySelector<HTMLInputElement>('input[type=checkbox]')!;expect(checkbox.checked).toBe(true);
 checkbox.click();await tick();expect(button('导入').disabled).toBe(true);
});
it('Keep 默认包含归档，失败后保留原因并可重试',async()=>{
 await start('keep');expect(document.querySelector('[role=switch]')?.getAttribute('aria-checked')).toBe('true');
 await message({type:'error',message:'Broken ZIP'});expect(document.querySelector('[role=alert]')?.textContent).toBe('Broken ZIP');
 expect(invoke).not.toHaveBeenCalledWith('finish_note_import',expect.anything());
});
it('更改位置时释放旧导入会话并重新准备',async()=>{
 await start('evernote');button('更改').click();await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('prepare_note_import',{source:'evernote',destination:'/Archive',applePath:null}));
 expect(invoke).toHaveBeenCalledWith('close_note_import',{session:123});
});
it('拒绝其他窗口发来的写文件请求',async()=>{
 await start('notion');window.dispatchEvent(new MessageEvent('message',{origin:location.origin,data:{channel:'sag-note-import',type:'native',command:'write_note_import_file',args:{session:123},request:2}}));
 await tick();expect(invoke).not.toHaveBeenCalledWith('write_note_import_file',expect.anything());
});
it('权限不足显示授权引导，打开系统设置后返回重试',async()=>{
 const normal=invoke.getMockImplementation()!;
 invoke.mockImplementation((command,...args)=>command==='prepare_note_import'?Promise.reject('APPLE_NOTES_PERMISSION_REQUIRED'):normal(command,...args));
 app=mount(NoteImport,{target:document.body,props:{source:'apple',onClose:vi.fn(),onImported:vi.fn()}});
 await vi.waitFor(()=>expect(button('打开系统设置')).toBeDefined());
 expect(document.querySelector('[role=alert]')).toBeNull();
 expect(document.body.textContent).not.toContain('没有找到备忘录文件夹');
 button('打开系统设置').click();await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_full_disk_access',undefined));
 invoke.mockImplementation(normal);window.dispatchEvent(new Event('focus'));
 await vi.waitFor(()=>expect(document.body.textContent).toContain('工作'));
 expect(button('打开系统设置')).toBeUndefined();
});
it('数据文件损坏保留错误，不显示权限引导',async()=>{
 invoke.mockRejectedValue('database disk image is malformed');
 app=mount(NoteImport,{target:document.body,props:{source:'apple',onClose:vi.fn(),onImported:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('database disk image is malformed'));
 expect(button('打开系统设置')).toBeUndefined();
});

it('全部笔记转换失败时不显示成功标记',async()=>{
 await start('apple');
 await message({type:'complete',notes:0,attachments:0,log:[{outcome:'failed',name:'Test note',reason:'Conversion failed'}]});
 await vi.waitFor(()=>expect(document.body.textContent).toContain('导入失败'));
 expect(document.querySelector('.import-result')).toBeNull();
});
