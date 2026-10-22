// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import type {ModelKind} from '../src/lib/providers';
import type {Settings} from '../src/lib/types';

const {api}=vi.hoisted(()=>({api:vi.fn()}));
vi.mock('../src/lib/api',()=>({api,listen:vi.fn(async()=>()=>{})}));
const value=()=>structuredClone({answer:{url:'https://api.302ai.cn/v1',model:'first',models:['first','second'],has_key:false},jev:{url:'https://api.302ai.cn/typesafeai/v1',model:'jev-latest',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:0} as unknown as Settings);
let app:ReturnType<typeof mount>|undefined;
const card=(kind:ModelKind)=>document.querySelectorAll<HTMLElement>('.model-settings-card')[kind==='answer'?0:1];
const key=(kind:ModelKind)=>card(kind).querySelector<HTMLInputElement>('input[type=password]');
const authTrigger=()=>card('jev').querySelector<HTMLButtonElement>('.auth-field button')!;
async function chooseAuth(id:'bearer'|'none'){
 authTrigger().dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector(`[role=option][data-value="${id}"]`)).not.toBeNull());
 document.querySelector(`[role=option][data-value="${id}"]`)!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));
 await tick();
}
async function input(element:HTMLInputElement,text:string){element.value=text;element.dispatchEvent(new Event('input',{bubbles:true}));await tick();}
async function setup(settings=value()){const onSave=vi.fn().mockResolvedValue(undefined);app=mount(SettingsPanel,{target:document.body,props:{value:settings,initial:'models',onClose:()=>{},onSave}});await tick();return onSave;}
async function save(){document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();}
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';api.mockReset();});

it('defaults an existing Jev configuration to API Key and hides the option for the answer model',async()=>{
 api.mockImplementation(async command=>{if(command==='get_access')return {enabled:false};return null;});
 await setup();
 expect(authTrigger().textContent).toContain('API Key');
 expect(card('answer').querySelector('.auth-field')).toBeNull();
 expect(key('jev')).not.toBeNull();
});

it('lets a local Jev run without authentication: tests with an empty key, saves the mode, and drops draft keys',async()=>{
 api.mockImplementation(async command=>{if(command==='get_access')return {enabled:false};if(command==='test_model')return '连接成功';return null;});
 const settings=value();settings.jev.has_key=true;
 const onSave=await setup(settings);
 await input(key('jev')!,'test-draft-jev');
 await chooseAuth('none');
 expect(key('jev')).toBeNull();
 card('jev').querySelector<HTMLButtonElement>('.settings-card-actions button')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:'jev',key:'',config:expect.objectContaining({auth_mode:'none'})})));
 await vi.waitFor(()=>expect(document.body.textContent).toContain('连接成功'));
 await save();
 await vi.waitFor(()=>expect(onSave).toHaveBeenCalledTimes(1));
 expect(onSave.mock.calls[0][0].jev.auth_mode).toBe('none');
 expect(onSave.mock.calls[0][2]).toBe('');
 const saved=structuredClone(onSave.mock.calls[0][0]);
 await unmount(app!);app=undefined;document.body.innerHTML='';
 await setup(saved);
 expect(authTrigger().textContent).toContain('无需鉴权');
 expect(key('jev')).toBeNull();
 await chooseAuth('bearer');
 expect(key('jev')!.value).toBe('');
});
