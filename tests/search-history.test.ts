// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import App from '../src/App.svelte';
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async()=>()=>{})}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
vi.mock('@tauri-apps/plugin-dialog',()=>({open:vi.fn()}));
let app:ReturnType<typeof mount>;
beforeEach(()=>{
 localStorage.removeItem('sag.search.history');
 invoke.mockImplementation(async command=>{
  if(command==='get_settings')return{theme:'light',start_page:'search',editor_size:15,answer:{model:'test',models:['test']}};
  if(command==='run_query')return{results:[],terms:[],trace:[],conversation:null};
  return[];
 });
});
afterEach(async()=>{await unmount(app);document.body.innerHTML='';localStorage.removeItem('sag.search.history');vi.clearAllMocks()});
async function start(){app=mount(App,{target:document.body});await vi.waitFor(()=>expect(document.querySelector('[aria-label="搜索关键词"]')).not.toBeNull())}
async function search(question:string){
 const input=document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!;input.value=question;input.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 input.closest('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('.search-page')!.getAttribute('aria-busy')).toBe('false'));
}
async function back(){document.querySelector<HTMLButtonElement>('.search-options-row .search-new-query')!.click();await vi.waitFor(()=>expect(document.querySelector('.search-page.has-results')).toBeNull())}
it('结果页返回时清空输入，并在输入框下显示最近搜索，点击可再次搜索',async()=>{
 await start();expect(document.querySelector('[aria-label="最近搜索"]')).toBeNull();
 await search('Token 路由器');
 expect(document.querySelector('.search-options-row .search-new-query')).not.toBeNull();
 expect(document.querySelector('.search-composer .search-new-query')).toBeNull();
 await back();
 expect(document.querySelector<HTMLInputElement>('[aria-label="搜索关键词"]')!.value).toBe('');
 expect(document.activeElement).toBe(document.querySelector('[aria-label="搜索关键词"]'));
 const history=document.querySelector('[aria-label="最近搜索"]')!;
 expect(history).not.toBeNull();expect(document.querySelector('.search-composer')!.compareDocumentPosition(history)&Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
 history.querySelector<HTMLButtonElement>('.search-history-item')!.click();
 await vi.waitFor(()=>expect(invoke.mock.calls.filter(([command])=>command==='run_query')).toHaveLength(2));
 expect(invoke.mock.calls.filter(([command])=>command==='run_query').at(-1)![1].query.question).toBe('Token 路由器');
});
it('历史去重，最近一次排在前面，最多保存八条',async()=>{
 await start();
 for(let i=0;i<9;i++){await search('问题 '+i);await back()}
 await search('问题 4');await back();
 const labels=[...document.querySelectorAll('.search-history-item')].map(button=>button.textContent?.trim());
 expect(labels).toEqual(['问题 4','问题 8','问题 7','问题 6','问题 5','问题 3','问题 2','问题 1']);
 expect(JSON.parse(localStorage.getItem('sag.search.history')!)).toEqual(labels);
});
it('重新打开应用后仍显示已保存的历史',async()=>{
 localStorage.setItem('sag.search.history',JSON.stringify(['部署说明','Token 路由器']));await start();
 expect([...document.querySelectorAll('.search-history-item')].map(button=>button.textContent?.trim())).toEqual(['部署说明','Token 路由器']);
});
it('点击清空图标删除全部历史，重新打开应用后仍为空',async()=>{
 localStorage.setItem('sag.search.history',JSON.stringify(['部署说明','Token 路由器']));await start();
 document.querySelector<HTMLButtonElement>('[aria-label="清空搜索历史"]')!.click();await tick();
 expect(document.querySelector('[aria-label="最近搜索"]')).toBeNull();
 await unmount(app);await start();
 expect(document.querySelector('[aria-label="最近搜索"]')).toBeNull();
 await search('新的搜索');await back();
 expect([...document.querySelectorAll('.search-history-item')].map(button=>button.textContent?.trim())).toEqual(['新的搜索']);
});
