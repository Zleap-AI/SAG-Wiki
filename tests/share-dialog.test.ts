// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import ShareDialog from '../src/components/ShareDialog.svelte';
const {api,copyText}=vi.hoisted(()=>({api:vi.fn(),copyText:vi.fn()}));
vi.mock('../src/lib/web-connection',()=>({copyText}));
vi.mock('../src/lib/api',()=>({api}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';api.mockReset();copyText.mockReset()});
const button=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()===text)!;
it('receives only after confirmation, using the configured directory',async()=>{
 api.mockImplementation(async(command)=>{
  if(command==='get_share_settings')return{directory:'/Documents/sag-wiki/share',clipboard:false};
  if(command==='receive_share')return{path:'/Documents/sag-wiki/share/Notes'};
 });
 const received=vi.fn();
 app=mount(ShareDialog,{target:document.body,props:{initialCode:'SAG-ABCDEFGH',onClose:vi.fn(),onReceived:received}});
 await vi.waitFor(()=>expect(button('下载').disabled).toBe(false));
 expect(api).not.toHaveBeenCalledWith('receive_share',expect.anything());
 button('下载').click();
 await vi.waitFor(()=>expect(received).toHaveBeenCalledWith('/Documents/sag-wiki/share/Notes'));
 expect(api).toHaveBeenCalledWith('receive_share',{code:'SAG-ABCDEFGH',directory:'/Documents/sag-wiki/share'});
 expect(document.body.textContent).toContain('下载完成');
 expect(api.mock.calls.every(([command])=>!['claim','complete'].includes(command))).toBe(true);
});
it('prepares locally and uploads once confirmed, then releases its temporary package',async()=>{
 api.mockImplementation(async(command)=>{
  if(command==='get_share_settings')return{directory:'/Documents/sag-wiki/share',clipboard:false};
  if(command==='prepare_share')return{id:'prepared',name:'Notes',markdown_count:2,image_count:1,size:1024};
  if(command==='upload_share')return{code:'ABCDEFGH',expiresAt:'2026-11-17T12:00:00Z'};
 });
 app=mount(ShareDialog,{target:document.body,props:{path:'/Notes',onClose:vi.fn(),onReceived:vi.fn()}});
 await vi.waitFor(()=>expect(document.body.textContent).toContain('2 篇文档'));
 expect(api).not.toHaveBeenCalledWith('upload_share',expect.anything());
 button('分享').click();await vi.waitFor(()=>expect(document.body.textContent).toContain('SAG-ABCDEFGH'));
 expect(api).toHaveBeenCalledWith('upload_share',{id:'prepared'});
 button('复制分享码').click();
 await vi.waitFor(()=>expect(copyText).toHaveBeenCalledWith('分享码：SAG-ABCDEFGH\n复制此分享码，打开SAG-Wiki即可下载对应文件内容'));
 await vi.waitFor(()=>expect(button('已复制')).toBeDefined());
 await unmount(app);app=mount(ShareDialog,{target:document.body,props:{onClose:vi.fn(),onReceived:vi.fn()}});
 expect(api).toHaveBeenCalledWith('close_share',{id:'prepared'});
});
it('keeps the code and reports download failures for retry',async()=>{
 api.mockImplementation(async(command)=>{
  if(command==='get_share_settings')return{directory:'/Documents/sag-wiki/share',clipboard:false};
  if(command==='receive_share')throw '分享不可用或已过期';
 });
 const received=vi.fn();
 app=mount(ShareDialog,{target:document.body,props:{initialCode:'SAG-ABCDEFGH',onClose:vi.fn(),onReceived:received}});
 await vi.waitFor(()=>expect(button('下载').disabled).toBe(false));button('下载').click();
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('分享不可用或已过期'));
 expect((document.querySelector('input') as HTMLInputElement).value).toBe('SAG-ABCDEFGH');
 expect(received).not.toHaveBeenCalled();expect(button('下载').disabled).toBe(false);
});

it('shares remain unrequested while the receive dialog is open',async()=>{
 api.mockResolvedValue({directory:'/Documents/sag-wiki/share',clipboard:false});
 const onClose=vi.fn();
 app=mount(ShareDialog,{target:document.body,props:{initialCode:'SAG-ABCDEFGH',onClose,onReceived:vi.fn()}});
 await vi.waitFor(()=>expect(button('下载').disabled).toBe(false));
 button('取消').click();await tick();
 expect(onClose).toHaveBeenCalledOnce();
 expect(api.mock.calls.map(([command])=>command)).toEqual(['get_share_settings']);
});

it('centers the code and tells recipients where to receive it',async()=>{
 api.mockImplementation(async(command)=>{
  if(command==='get_share_settings')return{directory:'/Documents/sag-wiki/share',clipboard:true};
  if(command==='prepare_share')return{id:'prepared',name:'Notes',markdown_count:1,image_count:0,size:10};
  if(command==='upload_share')return{code:'ABCDEFGH',expiresAt:'2026-11-17T12:00:00Z'};
 });
 app=mount(ShareDialog,{target:document.body,props:{path:'/Notes',onClose:vi.fn(),onReceived:vi.fn()}});
 await vi.waitFor(()=>expect(button('分享').disabled).toBe(false));button('分享').click();
 await vi.waitFor(()=>expect(document.querySelector('.share-result .share-code')?.textContent).toBe('SAG-ABCDEFGH'));
 expect(document.querySelector('.share-instructions')?.textContent).toContain('设置 → 数据源');
});

it.each([[180,'180 B'],[1024,'1.0 KB'],[1048576,'1.00 MB']])('displays %i bytes without rounding small files to zero',async(size,label)=>{
 api.mockImplementation(async(command)=>command==='prepare_share'?{id:'small',name:'Notes',markdown_count:1,image_count:0,size}:{directory:'/Documents/sag-wiki/share',clipboard:true});
 app=mount(ShareDialog,{target:document.body,props:{path:'/Notes',onClose:vi.fn(),onReceived:vi.fn()}});
 await vi.waitFor(()=>expect(document.body.textContent).toContain(label));
 expect(document.querySelector('.share-hint')?.textContent).toBe('分享有效期为 1 小时');
});

it('uses the current folder for this receipt without changing the saved default',async()=>{
 api.mockImplementation(async(command)=>{
  if(command==='get_share_settings')return{directory:'/Documents/sag-wiki/share',clipboard:true};
  if(command==='receive_share')return{path:'/Projects/Notes/Received'};
 });
 app=mount(ShareDialog,{target:document.body,props:{initialCode:'SAG-ABCDEFGH',initialDirectory:'/Projects/Notes',onClose:vi.fn(),onReceived:vi.fn()}});
 await vi.waitFor(()=>expect(button('下载').disabled).toBe(false));
 expect(document.querySelector('.share-path')?.textContent).toBe('/Projects/Notes');
 button('下载').click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('receive_share',{code:'SAG-ABCDEFGH',directory:'/Projects/Notes'}));
 expect(api.mock.calls.some(([command])=>command==='save_share_settings')).toBe(false);
});

it.each(['zh-CN','en'] as const)('opens the matching share-code guide in %s',async(locale)=>{
 const {setLanguage}=await import('../src/lib/i18n');setLanguage(locale);
 api.mockResolvedValue({directory:'/Documents/sag-wiki/share',clipboard:true});
 app=mount(ShareDialog,{target:document.body,props:{onClose:vi.fn(),onReceived:vi.fn()}});
 await vi.waitFor(()=>expect(document.querySelector('input')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.share-help')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('open_external_link',{url:`http://sag-wiki.zleap.com/${locale==='en'?'en':'zh'}/docs/file-sharing`}));
 setLanguage('zh-CN');
});


it.each([
 '分享码：SAG-YQJWWBTH 复制此分享码，打开SAG-Wiki即可下载对应文件内容',
 '分享码：SAG-YQJWWBTH\n复制此分享码，打开SAG-Wiki即可下载对应文件内容',
 'Share code: SAG-YQJWWBTH Copy this share code and open SAG-Wiki to download the shared files.',
])('extracts the code from a pasted share message: %s',async(message)=>{
 api.mockImplementation(async(command)=>command==='receive_share'?{path:'/Documents/sag-wiki/share/Notes'}:{directory:'/Documents/sag-wiki/share',clipboard:true});
 app=mount(ShareDialog,{target:document.body,props:{onClose:vi.fn(),onReceived:vi.fn()}});
 await vi.waitFor(()=>expect(button('下载').disabled).toBe(true));
 const input=document.querySelector('input')!;
 input.value=message;input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 expect(input.value).toBe('SAG-YQJWWBTH');
 button('下载').click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('receive_share',{code:'SAG-YQJWWBTH',directory:'/Documents/sag-wiki/share'}));
});

it('opens share history without uploading or discarding the prepared share',async()=>{
 api.mockImplementation(async command=>{
  if(command==='get_share_settings')return{directory:'/Documents/sag-wiki/share',clipboard:false};
  if(command==='prepare_share')return{id:'prepared',name:'Notes',markdown_count:1,image_count:0,size:10};
  if(command==='get_share_history')return[];
 });
 app=mount(ShareDialog,{target:document.body,props:{path:'/Notes',onClose:vi.fn(),onReceived:vi.fn()}});
 await vi.waitFor(()=>expect(button('分享记录')?.disabled).toBe(false));
 button('分享记录').click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('暂无分享记录'));
 expect(api).not.toHaveBeenCalledWith('upload_share',expect.anything());
 expect(api).not.toHaveBeenCalledWith('close_share',expect.anything());
 const history=document.querySelector('.share-history')!;
 const close=[...history.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()==='关闭')!;
 close.click();await tick();
 expect(document.querySelector('.share-history')).toBeNull();
 expect(button('分享').disabled).toBe(false);
});
