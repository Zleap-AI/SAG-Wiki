// @vitest-environment happy-dom
import {loadDocumentBridge} from './helpers/document-bridge';
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import {Window} from 'happy-dom';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const url='https://psycnet.apa.org/record/2010-07936-005';
const source={id:1,document_id:1,path:'/files/notes.html',title:'notes.html',heading:'',content:'参考资料',start_line:1,end_line:1,source_ranges:[[1,1]],source_spans:[],bm25:-1,score:0,order:0,hash:'same'};
let app:ReturnType<typeof mount>;
beforeEach(async()=>{
 Object.assign((window as unknown as Window).happyDOM.settings.navigation,{disableMainFrameNavigation:true,disableChildFrameNavigation:true,disableChildPageNavigation:true});
 invoke.mockImplementation(async(command)=>{
  if(command==='get_settings')return {theme:'light',start_page:'ask',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='list_conversations')return [{id:'links',title:'链接测试',messages:[{question:'参考资料',answer:`[论文](${url}) [1]`,evidence:[source],model:'test',scope:[],references:[],status:'done',error:null}],updated:0,deleted:false}];
  if(command==='read_document')return {path:source.path,name:source.title,body:`<a href="${url}" target="_top"><strong>论文</strong></a><a href="#chapter">章节</a><h2 id="chapter">参考资料</h2>`,hash:'same'};
  return [];
 });
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('[title="链接测试"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[title="链接测试"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.answer-text a')).not.toBeNull());
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.clearAllMocks()});
it('问答里的外部链接在系统浏览器打开，保留当前会话',async()=>{
 const link=document.querySelector<HTMLAnchorElement>('.answer-text a')!;
 const click=new MouseEvent('click',{bubbles:true,cancelable:true});link.dispatchEvent(click);
 expect(click.defaultPrevented).toBe(true);
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_external_link',{url}));
 expect(document.querySelector('.user-message')?.textContent).toBe('参考资料');
});
it('HTML 引用里的外部链接也在浏览器打开，章节链接留在文档内',async()=>{
 document.querySelector<HTMLButtonElement>('.source-chip')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.source-panel iframe')).not.toBeNull());
 const frame=document.querySelector<HTMLIFrameElement>('.source-panel iframe')!;
 const doc=new DOMParser().parseFromString(frame.getAttribute('srcdoc')!,'text/html');
 loadDocumentBridge(frame,doc);await tick();
 const click=new MouseEvent('click',{bubbles:true,cancelable:true});doc.querySelector('strong')!.dispatchEvent(click);
 expect(click.defaultPrevented).toBe(true);
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_external_link',{url}));
 invoke.mockClear();const anchorClick=new MouseEvent('click',{bubbles:true,cancelable:true});doc.querySelectorAll('a')[1].dispatchEvent(anchorClick);await tick();
 expect(invoke).not.toHaveBeenCalled();expect(document.querySelector('.source-panel iframe')).toBe(frame);
});
it('打开浏览器失败时只提示错误并保留会话',async()=>{
 invoke.mockRejectedValueOnce(new Error('无法在浏览器中打开链接'));
 document.querySelector<HTMLAnchorElement>('.answer-text a')!.dispatchEvent(new MouseEvent('click',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('#notice')?.textContent).toContain('无法在浏览器中打开链接'));
 expect(document.querySelector('.user-message')?.textContent).toBe('参考资料');
});

it('侧栏帮助在系统浏览器打开文档站，收起侧栏后仍可使用',async()=>{
 const help=()=>document.querySelector<HTMLButtonElement>('button[aria-label="打开帮助文档"]')!;
 expect(help().title).toBe('帮助文档');
 help().click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_external_link',{url:'http://sag-wiki.zleap.com'}));
 expect(document.querySelector('.user-message')?.textContent).toBe('参考资料');
 document.querySelector<HTMLButtonElement>('[aria-label="收起侧边栏"]')!.click();await tick();
 invoke.mockClear();help().click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('open_external_link',{url:'http://sag-wiki.zleap.com'}));
 expect(document.querySelector('.user-message')?.textContent).toBe('参考资料');
});
