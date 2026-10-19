// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import FolderPicker from '../src/components/FolderPicker.svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
let app:ReturnType<typeof mount>;
let fail=false;
const onAdd=vi.fn(async(paths:string[])=>fail?paths.filter(path=>path==='/home/docs').map(path=>({path,error:'没有权限'})):[]);
const onClose=vi.fn();
const contents:Record<string,{path:string;name:string}[]>={
 '/home':[{path:'/home/docs',name:'docs'},{path:'/home/projects',name:'projects'},{path:'/home/added',name:'added'}],
 '/home/projects':[{path:'/home/projects/client',name:'client'}],
};
beforeEach(()=>{fail=false;onAdd.mockClear();onClose.mockClear();invoke.mockImplementation(async(command,args)=>{
 if(command==='browse_folders'){
  const path=args.path??'/home';
  if(path==='/denied')throw Error('无法读取目录');
  return{path,parent:path==='/home'?'/':'/home',home:'/home',folders:contents[path]??[]};
 }
 throw Error(command);
})});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
async function start(){app=mount(FolderPicker,{target:document.body,props:{roots:['/home/added'],onAdd,onClose}});await vi.waitFor(()=>expect(document.querySelector('[aria-label="选择 docs"]')).not.toBeNull())}
async function click(label:string){document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!.click();await tick()}
it('应用内跨目录多选，一次提交所选文件夹，不自动加入浏览过的目录',async()=>{
 await start();
 expect(document.querySelector<HTMLButtonElement>('[aria-label="选择 added"]')!.disabled).toBe(true);
 await click('选择 docs');
 document.querySelector<HTMLButtonElement>('[title="/home/projects"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="选择 client"]')).not.toBeNull());
 await click('选择 client');
 expect(onAdd).not.toHaveBeenCalled();
 expect(document.querySelector('.folder-picker-footer')?.textContent).toContain('索引选项');
 expect(document.querySelector('.folder-picker-footer')?.textContent).not.toContain('已选');
 document.querySelector<HTMLButtonElement>('.folder-picker-footer .primary')!.click();
 await vi.waitFor(()=>expect(onAdd).toHaveBeenCalledWith(['/home/docs','/home/projects/client']));
 await vi.waitFor(()=>expect(onClose).toHaveBeenCalledOnce());
});
it('部分添加失败后显示原因，只保留失败项重试',async()=>{
 await start();fail=true;await click('选择 docs');await click('选择 projects');
 document.querySelector<HTMLButtonElement>('.folder-picker-footer .primary')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('没有权限'));
 expect(onClose).not.toHaveBeenCalled();
 expect(document.querySelector('.folder-picker-footer')?.textContent).toContain('索引选项');
 fail=false;document.querySelector<HTMLButtonElement>('.folder-picker-footer .primary')!.click();
 await vi.waitFor(()=>expect(onAdd).toHaveBeenLastCalledWith(['/home/docs']));
 await vi.waitFor(()=>expect(onClose).toHaveBeenCalledOnce());
});
it('支持输入路径，目录读取失败会提示且保留已选项',async()=>{
 await start();await click('选择 docs');
 const input=document.querySelector<HTMLInputElement>('[aria-label="文件夹路径"]')!;
 input.value='/denied';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role="alert"]')?.textContent).toContain('无法读取目录'));
 expect(document.querySelector('.folder-picker-selection')?.textContent).toContain('/home/docs');
});

it('知识库添加按钮打开应用内弹窗，并添加所有选中文件夹',async()=>{
 const roots:{path:string;name:string;status:string;count:number;error:null}[]=[];
 const browser=invoke.getMockImplementation()!;
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return{theme:'light',start_page:'ask',editor_size:15,show_hidden:true,answer:{model:'test',models:['test']}};
  if(command==='list_roots')return roots;
  if(command==='add_root'){roots.push({path:args.path,name:args.path.split('/').at(-1),status:'ready',count:0,error:null});return args.path}
  if(command==='browse_folders')return browser(command,args);
  return[];
 });
 localStorage.setItem('sag.onboarding.seen','1');
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="添加文件夹"]')).not.toBeNull());
 await click('添加文件夹');await vi.waitFor(()=>expect(document.querySelector('[aria-label="选择 docs"]')).not.toBeNull());
 expect(invoke).toHaveBeenCalledWith('browse_folders',{path:null,showHidden:false});
 expect(document.querySelector('.folder-picker-options label:first-child [role="checkbox"]')?.getAttribute('aria-checked')).toBe('false');
 await click('选择 docs');await click('选择 projects');
 document.querySelector<HTMLButtonElement>('.folder-picker-footer .primary')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.folder-picker')).toBeNull());
 expect(invoke.mock.calls.filter(([command])=>command==='add_root').map(([,args])=>args.path)).toEqual(['/home/docs','/home/projects']);
 expect(document.querySelector('.tree')?.textContent).toContain('docs');
 expect(document.querySelector('.tree')?.textContent).toContain('projects');
});


it('展开索引选项后可以为本次添加的文件夹指定独立类型',async()=>{
 await start();await click('选择 docs');
 document.querySelector<HTMLButtonElement>('.folder-index-toggle')!.click();
 await vi.waitFor(()=>expect(document.querySelector('#folder-type-code')).not.toBeNull());
 expect(document.querySelector('.folder-index-toggle')?.getAttribute('aria-expanded')).toBe('true');
 document.querySelector<HTMLButtonElement>('#folder-type-code')!.click();await tick();
 document.querySelector<HTMLButtonElement>('#folder-type-markdown')!.click();await tick();
 document.querySelector<HTMLButtonElement>('.folder-picker-footer .primary')!.click();
 await vi.waitFor(()=>expect(onAdd).toHaveBeenCalledWith(['/home/docs'],{file_types:{markdown:false,text:true,code:true,config:false},convert_other_formats:false}));
});
