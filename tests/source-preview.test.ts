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
const evidence={id:1,document_id:1,path:'/files/notes.md',title:'notes.md',heading:'',content:'原文片段',start_line:5,end_line:5,source_ranges:[[5,5]],source_spans:[],bm25:-1,score:0,order:0,hash:'same'};
const sources=[evidence,{...evidence,id:2,content:'同一文件的第二处原文',start_line:9,end_line:9},{...evidence,id:3,path:'/archive/notes.md',content:'副本中的原文',start_line:12,end_line:12},{...evidence,id:4,path:'/other/notes.md',hash:'different',content:'同名文件的不同内容'},{...evidence,id:5,path:'/unused.md',hash:'unused',content:'没有被引用的原文'}];
const conversation={id:'test',title:'引用测试',messages:[{question:'测试问题',answer:'答案 [3]，另一处 [2]，同名文件 [4]，再次引用 [3]',evidence:sources,model:'test',scope:[],references:[],status:'done',error:null,retrieval_question:'测试问题'}],updated:0,deleted:false};
const documents=Object.fromEntries(sources.map(source=>{const entries=sources.filter(item=>item.path===source.path),lines=Array.from({length:Math.max(...entries.map(item=>item.start_line))+2},()=> '');lines[0]='文档引言';lines[2]='# 完整文档';for(const item of entries)lines[item.start_line-1]=item.content;return[source.path,{path:source.path,name:source.title,body:lines.join('\n')+'\n\n引用之外的正文',hash:source.hash}]}));
beforeEach(async()=>{
 invoke.mockImplementation(async(command,args)=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return [conversation];
  if(command==='read_document')return documents[args.path];
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
 it('渲染 Markdown 全文并高亮引用段落',async()=>{
  await vi.waitFor(()=>expect(document.querySelector('.source-panel h1')?.textContent).toBe('完整文档'));
  expect(document.querySelector('.source-panel .search-preview-body')?.textContent).toContain('引用之外的正文');
  expect(document.querySelector('.source-panel .search-hit.active')?.textContent).toBe('副本中的原文');
  expect(document.querySelector('.source-panel')?.textContent).not.toContain('以上为回答使用的原文片段');
 });
 it('打开引用文档后返回原会话，保留未发送的输入',async()=>{
  const input=document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!;
  input.value='未发送的追问';input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
  await vi.waitFor(()=>expect(document.querySelector('.source-panel .panel-footer button')).not.toBeNull());
  document.querySelector<HTMLButtonElement>('.source-panel .panel-footer button')!.click();
  await vi.waitFor(()=>expect(document.querySelector('.wysiwyg-editor')).not.toBeNull());
  const back=document.querySelector<HTMLButtonElement>('[title="返回会话"]');expect(back).not.toBeNull();
  back!.click();await vi.waitFor(()=>expect(document.querySelector('.chat-messages')).not.toBeNull());
  expect(document.querySelector('.user-message')?.textContent).toBe('测试问题');
  expect(document.querySelector<HTMLTextAreaElement>('[aria-label="向你的文件提问"]')!.value).toBe('未发送的追问');
  expect(document.querySelector('.source-panel')).toBeNull();
 });
 it('从 HTML 引用打开文档后也返回原会话',async()=>{
  await unmount(app);
  const source={...evidence,path:'/files/notes.html',title:'notes.html',content:'<p>HTML 引用段落</p>'};
  const htmlConversation={...conversation,messages:[{...conversation.messages[0],answer:'答案 [1]',evidence:[source]}]};
  const base=invoke.getMockImplementation()!;
  invoke.mockImplementation((command,args)=>command==='list_conversations'?[htmlConversation]:command==='read_document'?{path:source.path,name:source.title,body:'<h1>完整 HTML</h1><p>HTML 引用段落</p><p>未引用的全文结尾</p>',hash:'same'}:base(command,args));
  app=mount(App,{target:document.body});
  await vi.waitFor(()=>expect(document.querySelector('[title="引用测试"]')).not.toBeNull());
  document.querySelector<HTMLButtonElement>('[title="引用测试"]')!.click();
  await vi.waitFor(()=>expect(document.querySelector('.source-chip')).not.toBeNull());
  document.querySelector<HTMLButtonElement>('.source-chip')!.click();
  await vi.waitFor(()=>expect(document.querySelector('.source-panel iframe')).not.toBeNull());
  document.querySelector<HTMLButtonElement>('.source-panel .panel-footer button')!.click();
  await vi.waitFor(()=>expect(document.querySelector('.readonly-document iframe')).not.toBeNull());
  document.querySelector<HTMLButtonElement>('[title="返回会话"]')!.click();
  await vi.waitFor(()=>expect(document.querySelector('.chat-messages')).not.toBeNull());
  expect(document.querySelector('.user-message')?.textContent).toBe('测试问题');
  expect(document.querySelector('.source-panel')).toBeNull();
 });
 it('仅显示正文引用的文件，合并副本并按出现顺序连续编号',()=>{
  const chips=[...document.querySelectorAll('.source-chip')];
  expect(chips).toHaveLength(2);
  expect(chips.map(chip=>chip.querySelector('small')!.textContent)).toEqual(['1','2']);
  expect(chips.map(chip=>chip.querySelector('span')!.textContent)).toEqual(['notes.md','notes.md']);
 });
 it('合并卡片后，正文的各个引用仍打开对应段落且不显示路径',async()=>{
  expect([...document.querySelectorAll('.answer-text .citation')].map(button=>button.textContent)).toEqual(['1','1','2','1']);
  for(const index of [2,1,3]){
   const source=sources[index];
   document.querySelector<HTMLButtonElement>('[aria-label="关闭引用"]')!.click();await tick();
   await vi.waitFor(()=>expect(document.querySelector(`.answer-text [data-citation="${index}"]`)).not.toBeNull());
   document.querySelector<HTMLButtonElement>(`.answer-text [data-citation="${index}"]`)!.click();await tick();
   await vi.waitFor(()=>expect(document.querySelector('.source-panel .search-hit.active')?.textContent).toBe(source.content));
   expect(document.querySelector('.source-panel .source-path')).toBeNull();
  }
  expect(conversation.messages[0].evidence).toHaveLength(5);
 });
 it('打开为模态弹层，点击左侧背景即可关闭',async()=>{
  expect(document.querySelector('[aria-label="引用原文"]')?.getAttribute('aria-modal')).toBe('true');
  await vi.waitFor(()=>expect(document.querySelector('.source-panel .search-preview-body')).not.toBeNull());
  document.querySelector<HTMLElement>('.source-panel .search-preview-body')!.click();await tick();
  expect(document.querySelector('[aria-label="引用原文"]')?.getAttribute('aria-modal')).toBe('true');
  document.querySelector('.source-dialog')!.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,pointerType:'mouse',clientX:1000,clientY:1000}));
  await vi.waitFor(()=>expect(document.querySelector('[aria-label="引用原文"]')).toBeNull());
 });
 it('按 Escape 收起弹层',async()=>{
  document.querySelector('[aria-label="引用原文"]')!.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
  await vi.waitFor(()=>expect(document.querySelector('[aria-label="引用原文"]')).toBeNull());
 });
});
