// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
import Onboarding from '../src/components/Onboarding.svelte';
import {setLanguage} from '../src/lib/i18n';

const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
const settings={language:'zh-CN',theme:'light',start_page:'ask',editor_size:15,answer:{url:'',model:'test',models:['test']},jev:{url:'',model:'jev'},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:2,top_k:10,chunk_chars:2200},excluded:[],show_hidden:false,watch:true};
let app:ReturnType<typeof mount>;
beforeEach(()=>{
 invoke.mockImplementation(async(command)=>command==='get_settings'?settings:[]);
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';vi.restoreAllMocks();vi.clearAllMocks()});
function click(label:string){const button=[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.getAttribute('aria-label')===label||b.textContent?.trim()===label);expect(button,label).toBeDefined();button!.click()}
function track(){
 const node=document.querySelector<HTMLDivElement>('.onboarding-track')!;
 Object.defineProperty(node,'clientWidth',{value:640,configurable:true});
 node.scrollTo=vi.fn((options)=>{if(typeof options==='object')node.scrollLeft=options.left??0;node.dispatchEvent(new Event('scroll'))});
 node.setPointerCapture=vi.fn();
 return node;
}
function current(){return document.querySelector('.onboarding-slide[data-current=true] h2')?.textContent}

it('remembers dismissal and closes settings when reopening the guide',async()=>{
 localStorage.removeItem('sag.onboarding.seen');
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.onboarding-dialog')).not.toBeNull());
 expect(current()).toBe('你的文件，就是知识库。');
 click('关闭教程');await tick();
 expect(localStorage.getItem('sag.onboarding.seen')).toBe('1');
 await unmount(app);document.body.innerHTML='';
 app=mount(App,{target:document.body});
 await vi.waitFor(()=>expect(document.querySelector('.welcome')).not.toBeNull());
 expect(document.querySelector('.onboarding-dialog')).toBeNull();
 click('设置');await tick();click('查看教程');await tick();
 expect(current()).toBe('你的文件，就是知识库。');
 expect(document.querySelector('.settings-dialog')).toBeNull();
 click('关闭教程');await tick();
 expect(document.querySelector('.settings-dialog')).toBeNull();
 expect(document.querySelector('.welcome')).not.toBeNull();
 expect(invoke.mock.calls.some(([name])=>['save_settings','add_root','run_query','save_document'].includes(name))).toBe(false);
});

it('supports navigation, keyboard boundaries, native swiping, and mouse dragging',async()=>{
 app=mount(Onboarding,{target:document.body,props:{onClose:vi.fn()}});await tick();
 const node=track();
 click('下一步');await tick();expect(current()).toBe('顺着线索，找到更多。');
 window.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await tick();expect(current()).toBe('答案清楚，出处明确。');
 click('上一步');await tick();expect(current()).toBe('顺着线索，找到更多。');
 node.scrollLeft=3*640;node.dispatchEvent(new Event('scroll'));await tick();expect(current()).toBe('最轻。最快。最准。');
 window.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await tick();expect(current()).toBe('最轻。最快。最准。');
 click('教程第 1 页');await tick();
 node.dispatchEvent(new PointerEvent('pointerdown',{pointerType:'mouse',button:0,clientX:500,pointerId:1,bubbles:true}));
 node.dispatchEvent(new PointerEvent('pointermove',{pointerType:'mouse',clientX:350,pointerId:1,bubbles:true}));
 node.dispatchEvent(new PointerEvent('pointerup',{pointerType:'mouse',clientX:350,pointerId:1,bubbles:true}));await tick();
 expect(current()).toBe('顺着线索，找到更多。');
 expect(document.querySelectorAll('.onboarding-slide:not([inert])')).toHaveLength(1);
 expect(node.scrollTo).toHaveBeenLastCalledWith({left:640,behavior:'instant'});
});

it('finishes without making requests and translates all slides to English',async()=>{
 setLanguage('en');const onClose=vi.fn();
 app=mount(Onboarding,{target:document.body,props:{onClose}});await tick();track();
 expect(document.querySelector('.onboarding-dialog')!.textContent).not.toMatch(/[\u3400-\u9fff]/);
 click('Tutorial page 4');await tick();click('Get started');
 expect(onClose).toHaveBeenCalledOnce();expect(invoke).not.toHaveBeenCalled();
});

it.each([
 [1,'查看建库速度依据','208 毫秒','顺着线索，找到更多。'],
 [2,'SOTA 召回 · 论文基准','HotpotQA','答案清楚，出处明确。'],
])('keeps benchmark details collapsed on slide %s and closes them on navigation',async(page,trigger,detail,nextTitle)=>{
 app=mount(Onboarding,{target:document.body,props:{onClose:vi.fn()}});await tick();track();
 click(`教程第 ${page} 页`);await tick();
 expect(document.querySelector('.onboarding-benchmark-popover')).toBeNull();
 if(page===1){
  document.querySelector('[aria-label="查看建库速度依据"]')!.dispatchEvent(new PointerEvent('pointerenter',{pointerType:'mouse'}));
 }else click(trigger);
 await vi.waitFor(()=>expect(document.querySelector('.onboarding-benchmark-popover')?.textContent).toContain(detail));
 window.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true}));await tick();
 expect(current()).toBe(nextTitle);
 expect(document.querySelector('.onboarding-benchmark-popover')).toBeNull();
});

it('opens indexing details on click and closes them with Escape',async()=>{
 app=mount(Onboarding,{target:document.body,props:{onClose:vi.fn()}});await tick();track();
 const trigger=document.querySelector<HTMLButtonElement>('[aria-label="查看建库速度依据"]')!;
 trigger.dispatchEvent(new PointerEvent('pointerdown',{pointerType:'mouse',button:0,bubbles:true}));
 trigger.dispatchEvent(new PointerEvent('pointerup',{pointerType:'mouse',button:0,bubbles:true}));
 trigger.click();
 await vi.waitFor(()=>expect(document.querySelector('.onboarding-benchmark-popover')?.textContent).toContain('208 毫秒'));
 expect(trigger.textContent).toBe('建库约 200 毫秒 / 百万字');
 trigger.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('.onboarding-benchmark-popover')).toBeNull());
});
