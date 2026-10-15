// @vitest-environment happy-dom
import {afterEach,beforeEach,describe,it,expect,vi} from 'vitest';
import {mount,unmount,tick} from 'svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
const evidence={id:1,document_id:1,path:'/files/notes.md',title:'notes.md',heading:'',content:'原文片段',start_line:3,end_line:3,source_ranges:[[3,3]],source_spans:[],bm25:-1,score:0,order:0,hash:'same'};
const conversation={id:'test',title:'引用测试',messages:[{question:'测试问题',answer:'答案 [1]',evidence:[evidence],model:'test',scope:[],references:[],status:'done',error:null,retrieval_question:'测试问题'}],updated:0,deleted:false};
beforeEach(async()=>{
 invoke.mockImplementation(async(command)=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return [conversation];
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[title="引用测试"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[title="引用测试"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.source-chip')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('.source-chip')!.click();await vi.waitFor(()=>expect(document.querySelector('[role=dialog][aria-label="引用原文"]')).not.toBeNull());
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
describe('引用原文弹出层',()=>{
 it('打开为模态弹层，点击左侧背景即可关闭',async()=>{
  expect(document.querySelector('[aria-label="引用原文"]')?.getAttribute('aria-modal')).toBe('true');
  document.querySelector<HTMLElement>('.source-text')!.click();await tick();
  expect(document.querySelector('[aria-label="引用原文"]')?.getAttribute('aria-modal')).toBe('true');
  document.querySelector('.source-dialog')!.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,pointerType:'mouse',clientX:1000,clientY:1000}));
  await vi.waitFor(()=>expect(document.querySelector('[aria-label="引用原文"]')).toBeNull());
 });
 it('按 Escape 收起弹层',async()=>{
  document.querySelector('[aria-label="引用原文"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
  await vi.waitFor(()=>expect(document.querySelector('[aria-label="引用原文"]')).toBeNull());
 });
});
