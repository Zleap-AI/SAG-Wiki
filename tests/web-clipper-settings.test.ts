// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount} from 'svelte';
import WebClipper from '../src/components/WebClipper.svelte';
import {setLanguage} from '../src/lib/i18n';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let component:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(component);document.body.innerHTML='';vi.resetAllMocks();setLanguage('zh-CN');});
it('downloads extensions without pairing and adds a folder only when requested',async()=>{
 invoke.mockImplementation(async command=>{
  if(command==='get_web_clipper')return {packages:['chrome','safari']};
  if(command==='export_web_clipper')return '/Downloads/sag-web-clipper-safari.dmg';
  if(command==='add_web_clipper_folder')return '/Downloads/网页剪辑';
  return null;
 });
 component=mount(WebClipper,{target:document.body});
 const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()===text)!;
 await vi.waitFor(()=>expect(button('下载 Chrome / Edge 扩展')).toBeDefined());
 expect(document.querySelector('[role="switch"]')).toBeNull();
 expect(button('复制连接码')).toBeUndefined();
 expect(button('下载 Chrome / Edge 扩展').disabled).toBe(false);
 expect(button('下载 Safari 扩展').disabled).toBe(false);
 expect(invoke.mock.calls.map(call=>call[0])).toEqual(['get_web_clipper']);
 expect(document.body.textContent).toContain('Safari：打开 DMG');
 button('下载 Safari 扩展').click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('/Downloads/sag-web-clipper-safari.dmg'));
 expect(invoke).toHaveBeenCalledWith('export_web_clipper',{browser:'safari'});
 button('添加剪藏文件夹').click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('已添加剪藏目录：/Downloads/网页剪辑'));
 expect(invoke).toHaveBeenCalledWith('add_web_clipper_folder',undefined);
});
