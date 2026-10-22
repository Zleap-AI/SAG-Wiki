// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import RichEditor from '../src/components/RichEditor.svelte';

let app:ReturnType<typeof mount>|undefined;
let width=1500;
const resizeCallbacks=new Set<(width:number)=>void>();
const body='# 项目总览\n\n## 项目链接\n\n### 执行细节\n\n## 项目链接\n\n```md\n# 代码里的标题\n```\n\n正文';
beforeEach(()=>{
 width=1500;
 vi.stubGlobal('ResizeObserver',class{
  callback:(width:number)=>void;
  constructor(callback:ResizeObserverCallback){this.callback=width=>callback([{contentRect:{width}} as ResizeObserverEntry],this as unknown as ResizeObserver);resizeCallbacks.add(this.callback)}
  observe(){this.callback(width)}
  disconnect(){resizeCallbacks.delete(this.callback)}
 });
 vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockImplementation(function(this:HTMLElement){
  const headings=[...document.querySelectorAll('.tiptap h1,.tiptap h2,.tiptap h3')];
  const index=headings.indexOf(this);
  const top=index<0?50:72+index*240-(document.querySelector('.document-sheet')?.scrollTop??0);
  return{top,bottom:top+24,left:0,right:800,width:800,height:24,x:0,y:top,toJSON(){}};
 });
 vi.spyOn(HTMLElement.prototype,'clientHeight','get').mockReturnValue(600);
 vi.spyOn(HTMLElement.prototype,'scrollHeight','get').mockReturnValue(1800);
 vi.spyOn(HTMLElement.prototype,'scrollTo').mockImplementation(function(this:HTMLElement,options:any){this.scrollTop=options.top;this.dispatchEvent(new Event('scroll'))});
});
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';vi.restoreAllMocks();vi.unstubAllGlobals();resizeCallbacks.clear()});
async function start(content=body){
 const onChange=vi.fn();app=mount(RichEditor,{target:document.body,props:{body:content,path:'/notes/doc.md',onChange}});
 await tick();return onChange;
}
function resize(next:number){width=next;resizeCallbacks.forEach(callback=>callback(next))}

it('宽屏默认展开，按标题层级缩进，重复标题分别保留，排除代码块内容',async()=>{
 await start();
 await vi.waitFor(()=>expect(document.querySelectorAll('.outline-heading')).toHaveLength(4));
 expect(document.querySelector('[aria-label="收起文档目录"]')?.getAttribute('aria-expanded')).toBe('true');
 const headings=[...document.querySelectorAll<HTMLElement>('.outline-heading')];
 expect(headings.map(heading=>heading.textContent)).toEqual(['项目总览','项目链接','执行细节','项目链接']);
 expect(headings.map(heading=>heading.style.getPropertyValue('--heading-depth'))).toEqual(['0','1','2','1']);
});
it('窄屏默认折叠，未手动操作前跟随宽度，手动折叠后保持选择',async()=>{
 width=800;await start();
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="展开文档目录"]')).not.toBeNull());
 expect(document.querySelector<HTMLDivElement>('.outline-list')!.hidden).toBe(true);
 resize(1500);await tick();
 document.querySelector<HTMLButtonElement>('[aria-label="收起文档目录"]')!.click();await tick();
 resize(1600);await tick();
 expect(document.querySelector('[aria-label="展开文档目录"]')?.getAttribute('aria-expanded')).toBe('false');
 document.querySelector<HTMLButtonElement>('[aria-label="展开文档目录"]')!.click();await tick();
 expect(document.querySelector<HTMLDivElement>('.outline-list')!.hidden).toBe(false);
});
it('点击同名标题跳到对应位置，滚动更新当前标题，不触发文档修改',async()=>{
 const onChange=await start();
 await vi.waitFor(()=>expect(document.querySelectorAll('.outline-heading')).toHaveLength(4));
 (document.querySelectorAll('.outline-heading')[3] as HTMLButtonElement).click();await tick();
 const scroller=document.querySelector<HTMLElement>('.document-sheet')!;
 expect(scroller.scrollTop).toBe(720);
 expect(document.querySelector('.outline-heading[aria-current="location"]')?.textContent).toBe('项目链接');
 expect(document.querySelectorAll('.outline-heading')[3].getAttribute('aria-current')).toBe('location');
 scroller.scrollTop=480;scroller.dispatchEvent(new Event('scroll'));await tick();
 expect(document.querySelector('.outline-heading[aria-current="location"]')?.textContent).toBe('执行细节');
 expect(onChange).not.toHaveBeenCalled();
});
it('编辑标题文字后目录同步更新',async()=>{
 await start();
 await vi.waitFor(()=>expect(document.querySelectorAll('.outline-heading')).toHaveLength(4));
 document.querySelector('.tiptap h1')!.textContent='更新后的项目标题';
 await vi.waitFor(()=>expect(document.querySelector('.outline-heading')?.textContent).toBe('更新后的项目标题'));
});
it('没有标题时不显示目录或占用目录宽度',async()=>{
 await start('只有正文，没有标题。');await tick();
 expect(document.querySelector('.document-outline')).toBeNull();
 expect(document.querySelector('.tiptap')?.textContent).toBe('只有正文，没有标题。');
});
it('目录支持全部六级 Markdown 标题',async()=>{
 await start(Array.from({length:6},(_,index)=>'#'.repeat(index+1)+' 标题 '+(index+1)).join('\n\n'));
 await vi.waitFor(()=>expect(document.querySelectorAll('.outline-heading')).toHaveLength(6));
 expect([...document.querySelectorAll<HTMLElement>('.outline-heading')].map(heading=>heading.style.getPropertyValue('--heading-depth'))).toEqual(['0','1','2','3','4','5']);
});
