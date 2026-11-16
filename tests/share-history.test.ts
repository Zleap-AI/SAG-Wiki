// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,unmount} from 'svelte';
import ShareHistory from '../src/components/ShareHistory.svelte';
const {api}=vi.hoisted(()=>({api:vi.fn()}));
vi.mock('../src/lib/api',()=>({api}));
vi.mock('../src/lib/web-connection',()=>({copyText:vi.fn()}));
let app:ReturnType<typeof mount>;
afterEach(async()=>{await unmount(app);document.body.innerHTML='';api.mockReset()});
const button=(label:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.trim()===label)!;
const record={code:'ABCDEFGH',name:'Notes',shared_at:1700000000,expires_at:'2099-01-01T00:00:00Z',cancelled:false};
it('removes a cancelled share and keeps it absent when history is reopened',async()=>{
 let history=[record];
 api.mockImplementation(async command=>{if(command==='get_share_history')return history;if(command==='cancel_share')history=[]});
 app=mount(ShareHistory,{target:document.body,props:{onClose:vi.fn()}});
 await vi.waitFor(()=>expect(document.body.textContent).toContain('SAG-ABCDEFGH'));
 button('取消分享').click();
 expect(api).not.toHaveBeenCalledWith('cancel_share',expect.anything());
 await vi.waitFor(()=>expect(button('确认取消')).toBeDefined());button('确认取消').click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('cancel_share',{code:'ABCDEFGH'}));
 await vi.waitFor(()=>expect(document.body.textContent).toContain('暂无分享记录'));
 expect(document.body.textContent).not.toContain('SAG-ABCDEFGH');
 expect(button('取消分享')).toBeUndefined();
 await unmount(app);
 app=mount(ShareHistory,{target:document.body,props:{onClose:vi.fn()}});
 await vi.waitFor(()=>expect(document.body.textContent).toContain('暂无分享记录'));
 expect(document.body.textContent).not.toContain('SAG-ABCDEFGH');
});
it('reports cancellation failure without showing a cancelled state',async()=>{
 api.mockImplementation(async command=>{if(command==='get_share_history')return[record];if(command==='cancel_share')throw '分享服务请求失败，请稍后重试'});
 app=mount(ShareHistory,{target:document.body,props:{onClose:vi.fn()}});
 await vi.waitFor(()=>expect(button('取消分享')).toBeDefined());button('取消分享').click();
 await vi.waitFor(()=>expect(button('确认取消')).toBeDefined());button('确认取消').click();
 await vi.waitFor(()=>expect(document.querySelector('[role=alert]')?.textContent).toContain('分享服务请求失败'));
 expect(button('取消分享')).toBeDefined();
});
it('reloads persisted history when a record expires',async()=>{
 api.mockResolvedValueOnce([{...record,expires_at:'2020-01-01T00:00:00Z'}]).mockResolvedValue([]);
 app=mount(ShareHistory,{target:document.body,props:{onClose:vi.fn()}});
 await vi.waitFor(()=>expect(document.body.textContent).toContain('暂无分享记录'),{timeout:2500});
 expect(document.body.textContent).not.toContain('SAG-ABCDEFGH');
 expect(button('清空记录')).toBeUndefined();
 expect(button('取消分享')).toBeUndefined();
});
