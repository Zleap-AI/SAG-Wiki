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
const sources=[evidence,{...evidence,id:2,content:'同一文件的第二处原文',start_line:9,end_line:9},{...evidence,id:3,path:'/archive/notes.md',content:'副本中的原文',start_line:12,end_line:12},{...evidence,id:4,path:'/other/notes.md',hash:'different',content:'同名文件的不同内容'}];
const conversation={id:'test',title:'引用测试',messages:[{question:'测试问题',answer:'答案 [1]，另一处 [2]，副本 [3]，同名文件 [4]',evidence:sources,model:'test',scope:[],references:[],status:'done',error:null,retrieval_question:'测试问题'}],updated:0,deleted:false};
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
 it('合并同一文件的段落与完全相同的副本，保留内容不同的同名文件',()=>{
  const chips=[...document.querySelectorAll('.source-chip')];
  expect(chips).toHaveLength(2);
  expect(chips.map(chip=>chip.querySelector('small')!.textContent)).toEqual(['1, 2, 3','4']);
  expect(chips.map(chip=>chip.querySelector('span')!.textContent)).toEqual(['notes.md','notes.md']);
 });
 it('合并卡片后，正文的各个引用仍打开对应段落和路径',async()=>{
  for(const [index,source] of sources.entries()){
   document.querySelector<HTMLButtonElement>('[aria-label="关闭引用"]')!.click();await tick();
   await vi.waitFor(()=>expect(document.querySelector(`.answer-text [data-citation="${index}"]`)).not.toBeNull());
   document.querySelector<HTMLButtonElement>(`.answer-text [data-citation="${index}"]`)!.click();await tick();
   expect(document.querySelector('.source-text')!.textContent).toBe(source.content);
   expect(document.querySelector('.context-body .source-path')!.textContent).toContain(source.path);
  }
  expect(conversation.messages[0].evidence).toHaveLength(4);
 });
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
