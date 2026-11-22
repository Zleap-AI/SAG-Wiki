// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import DataImport from '../src/components/DataImport.svelte';

const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('@tauri-apps/api/core',()=>({invoke}));
let mounted:ReturnType<typeof mount>|undefined;
afterEach(async()=>{
 if(mounted)await unmount(mounted);
 mounted=undefined;
 document.body.innerHTML='';
 vi.resetAllMocks();
});

function button(label:string){
 return [...document.querySelectorAll<HTMLButtonElement>('button')].find(element=>
  element.getAttribute('aria-label')===label||element.textContent?.trim()===label);
}

it('scans Claude Code chat history without a delivery directory',async()=>{
 const report={changed:0,failed:0,errors:[]};
 const options={sources:[{agent:'claude-code',enabled:true,directory:'/old-output-folder'}],formats:['markdown'],destination:'/archive'};
 invoke.mockImplementation(async(command,{action})=>{
  expect(command).toBe('agent_artifacts');
  if(action.type==='status')return {running:false,options,detected:[{directories:[]}],report};
  if(action.type==='scan')return {scan:'chat',total:0,page:0,page_size:50,candidates:[],report};
 });
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false}});
 await tick();
 button('导入 Claude Code 产物')!.click();
 await vi.waitFor(()=>expect(document.querySelector('.agent-artifacts')?.textContent).toContain('没有找到可导入的交付文件'));
 expect(invoke).toHaveBeenCalledWith('agent_artifacts',{action:{type:'scan',options:{...options,sources:[{...options.sources[0],directory:null}]}}});
 expect(button('选择 Claude Code 的交付目录')).toBeUndefined();
 expect(button('重新扫描')?.disabled).toBe(false);
});

it('keeps selection across pages and enables automatic sync only through the switch',async()=>{
 const report={changed:0,failed:0,errors:[]};
 const sources=[{agent:'codex',enabled:true,directory:'/outputs/codex'}];
 const formats=['html','slides','word','spreadsheet','pdf','markdown','image'];
 const item=(index:number)=>({id:String(index),agent:'codex',path:'/outputs/codex/report-'+index+'.pdf',name:'report-'+index+'.pdf',format:'pdf',bundle:false});
 const page=(index:number)=>({
  scan:'preview',total:51,page:index,page_size:50,report,
  candidates:index?[item(50)]:Array.from({length:50},(_,n)=>item(n)),
 });
 invoke.mockImplementation(async(command,{action})=>{
  expect(command).toBe('agent_artifacts');
  switch(action.type){
   case 'status':
    expect(action.agent).toBe('codex');
    return {running:false,options:{sources,formats,destination:'/archive'},detected:[{agent:'codex',directories:[]}],report};
   case 'scan':
    expect(action.options.sources).toEqual(sources);
    return page(0);
   case 'page':return page(action.page);
   case 'start':return {...report,changed:50};
   case 'stop':return null;
  }
 });
 const imported=vi.fn().mockResolvedValue(undefined);
 mounted=mount(DataImport,{target:document.body,props:{showSharing:false,onImported:imported}});
 await tick();
 expect(button('导入 Claude Desktop 聊天记录')).toBeDefined();
 expect(button('导入 WorkBuddy 聊天记录')).toBeDefined();
 expect(button('导入 WorkBuddy 产物')).toBeDefined();
 button('导入 Codex 产物')!.click();
 await vi.waitFor(()=>expect(button('/outputs/codex/report-0.pdf')?.getAttribute('aria-checked')).toBe('true'));
 expect(button('自动同步')?.getAttribute('aria-checked')).toBe('false');
 expect(button('Markdown')?.getAttribute('aria-checked')).toBe('true');
 button('/outputs/codex/report-0.pdf')!.click();
 await tick();
 button('下一页')!.click();
 await vi.waitFor(()=>expect(button('/outputs/codex/report-50.pdf')?.getAttribute('aria-checked')).toBe('true'));
 document.querySelector<HTMLButtonElement>('.agent-artifacts footer button.primary')!.click();
 await vi.waitFor(()=>expect(imported).toHaveBeenCalledTimes(1));
 expect(invoke).toHaveBeenCalledWith('agent_artifacts',{action:{type:'start',scan:'preview',exclude_all:false,exceptions:['0'],auto_sync:false}});
 expect(button('自动同步')?.getAttribute('aria-checked')).toBe('false');
 button('自动同步')!.click();
 await vi.waitFor(()=>expect(imported).toHaveBeenCalledTimes(2));
 expect(invoke).toHaveBeenCalledWith('agent_artifacts',{action:{type:'start',scan:'preview',exclude_all:false,exceptions:[],auto_sync:true}});
 expect(button('自动同步')?.getAttribute('aria-checked')).toBe('true');
 button('自动同步')!.click();
 await vi.waitFor(()=>expect(invoke).toHaveBeenCalledWith('agent_artifacts',{action:{type:'stop',agent:'codex'}}));
 expect(button('自动同步')?.getAttribute('aria-checked')).toBe('false');
});
