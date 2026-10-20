// @vitest-environment happy-dom
import {afterEach, expect, it, vi} from 'vitest';
import {mount, tick, unmount} from 'svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import {providerFor, providers, sameProviderOrigin, type ModelKind} from '../src/lib/providers';
import type {Settings} from '../src/lib/types';

const {api} = vi.hoisted(()=>({api:vi.fn()}));
vi.mock('../src/lib/api',()=>({api}));
const value:Settings={answer:{url:'https://api.302.ai/v1',model:'first',models:['first','second'],has_key:false},jev:{url:'https://api.302ai.cn/typesafeai/v1',model:'jev-latest',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:0};
let app:ReturnType<typeof mount>|undefined;
const card=(kind:ModelKind)=>document.querySelectorAll<HTMLElement>('.model-settings-card')[kind==='answer'?0:1];
const address=(kind:ModelKind)=>card(kind).querySelector<HTMLInputElement>('input[type=url]')!;
const key=(kind:ModelKind)=>card(kind).querySelector<HTMLInputElement>('input[type=password]')!;
const trigger=(kind:ModelKind)=>card(kind).querySelector<HTMLButtonElement>('.provider-field button')!;
async function input(element:HTMLInputElement,text:string){element.value=text;element.dispatchEvent(new Event('input',{bubbles:true}));await tick();}
async function choose(kind:ModelKind,id:string){
 trigger(kind).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector(`[role=option][data-value="${id}"]`)).not.toBeNull());
 document.querySelector(`[role=option][data-value="${id}"]`)!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));
 await tick();
}
async function setup(settings=value){const onSave=vi.fn().mockResolvedValue(undefined);app=mount(SettingsPanel,{target:document.body,props:{value:structuredClone(settings),initial:'models',onClose:()=>{},onSave}});await tick();return onSave;}
async function save(){document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();}
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';api.mockReset();});

it('已有 302 配置默认显示 302，打开设置不覆盖地址',async()=>{
 await setup();expect(trigger('answer').textContent).toContain('302.AI');expect(trigger('jev').textContent).toContain('302.AI');
 expect(address('answer').value).toBe(value.answer.url);expect(address('jev').value).toBe(value.jev.url);expect(api).not.toHaveBeenCalled();
});
it('302 密钥入口打开指定网页，切换供应商后隐藏',async()=>{
 await setup();
 const link=()=>card('answer').querySelector<HTMLAnchorElement>('.settings-key-link');
 expect(link()?.textContent).toBe('获取 API Key');
 expect(link()?.href).toBe('https://302.ai/api-keys/list');
 link()!.click();await tick();
 expect(api).toHaveBeenCalledWith('open_about_link',{target:'302-api-keys'});
 await choose('answer','openai');expect(link()).toBeNull();
 expect(card('jev').querySelector('.settings-key-link')).not.toBeNull();
});
it('LLM 六个供应商自动填入地址，自定义地址与模型列表可以保存',async()=>{
 const onSave=await setup();
 for(const provider of providers.answer.filter(provider=>provider.value!=='custom')){await choose('answer',provider.value);expect(address('answer').value).toBe(provider.value==='302'?value.answer.url:provider.url);}
 await choose('answer','custom');await input(address('answer'),'https://proxy.example/v1');await save();
 expect(onSave.mock.calls[0][0].answer).toMatchObject({url:'https://proxy.example/v1',model:'first',models:['first','second']});
});
it('Jev 预设供应商填入对应评分接口，并提供自定义选项',async()=>{
 expect(providers.jev.map(provider=>provider.label)).toEqual(['302.AI','OpenRouter','Typesafe','自定义']);
 await setup();
 for(const provider of providers.jev.filter(provider=>provider.value!=='custom')){await choose('jev',provider.value);expect(address('jev').value).toBe(provider.url);}
 trigger('jev').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelectorAll('[role=option]')).toHaveLength(4));
 expect(document.querySelector('[role=option][data-value=custom]')).not.toBeNull();
});
it('Jev 自定义地址用于测试连接，保存后重新打开仍显示该地址',async()=>{
 const onSave=await setup();await choose('jev','custom');
 await input(address('jev'),'https://scorer.example/v1/decisions');
 expect(trigger('jev').textContent).toContain('自定义');
 await input(key('jev'),'test-custom-key');api.mockResolvedValue('连接成功');
 card('jev').querySelector<HTMLButtonElement>('.settings-card-actions button')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:'jev',key:'test-custom-key',config:expect.objectContaining({url:'https://scorer.example/v1/decisions'})})));
 await save();expect(onSave).toHaveBeenCalledTimes(1);
 const saved=structuredClone(onSave.mock.calls[0][0]);expect(saved.jev.url).toBe('https://scorer.example/v1/decisions');
 await unmount(app!);app=undefined;document.body.innerHTML='';await setup(saved);
 expect(trigger('jev').textContent).toContain('自定义');expect(address('jev').value).toBe(saved.jev.url);
});
it('原有非预设地址不丢失，Jev 显示自定义',async()=>{
 const settings=structuredClone(value);settings.answer.url='https://proxy.example/chat';settings.jev.url='https://scorer.example/systemone';
 const onSave=await setup(settings);expect(trigger('answer').textContent).toContain('自定义');expect(trigger('jev').textContent).toContain('自定义');
 await save();expect(onSave.mock.calls[0][0].jev.url).toBe(settings.jev.url);
});
it('供应商切换不会将已保存或已输入的另一家密钥用于测试或保存',async()=>{
 const settings=structuredClone(value);settings.answer.has_key=true;const onSave=await setup(settings);
 await choose('answer','openai');expect(key('answer').placeholder).toBe('填写 API 密钥');
 card('answer').querySelector<HTMLButtonElement>('.settings-card-actions button')!.click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('切换供应商后，请填写对应的 API 密钥。'));expect(api).not.toHaveBeenCalled();
 await save();expect(onSave).not.toHaveBeenCalled();
 await input(key('answer'),'test-openai-key');await choose('answer','302');expect(key('answer').value).toBe('');
 await choose('answer','openai');await input(key('answer'),'test-new-key');api.mockResolvedValue('连接成功');
 card('answer').querySelector<HTMLButtonElement>('.settings-card-actions button')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:'answer',key:'test-new-key',config:expect.objectContaining({url:'https://api.openai.com/v1'})})));
 await save();expect(onSave.mock.calls[0][1]).toBe('test-new-key');
});
it('供应商识别兼容已有地址，域名相似的自定义服务不会误认',()=>{
 expect(providerFor('answer','https://api.302ai.cn/v1/')).toBe('302');expect(providerFor('jev','https://openrouter.ai/api/alpha/decisions')).toBe('openrouter');
 expect(providerFor('answer','https://api.302.ai.other.example/v1')).toBe('custom');expect(providerFor('jev','invalid')).toBe('custom');
 expect(sameProviderOrigin('https://api.302.ai/v1','https://api.302.ai/v1/')).toBe(true);
 expect(sameProviderOrigin('https://api.302.ai/v1','https://api.openai.com/v1')).toBe(false);
});
it.each(['answer','jev'] as const)('两项均为 302 时，填写 %s 密钥会同步补齐空白项',async(kind)=>{
 const onSave=await setup();const other=kind==='answer'?'jev':'answer';
 await input(key(kind),'test-');expect(key(other).value).toBe('test-');
 await input(key(kind),'test-shared-key');expect(key(other).value).toBe('test-shared-key');
 api.mockResolvedValue('连接成功');card(other).querySelector<HTMLButtonElement>('.settings-card-actions button')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:other,key:'test-shared-key'})));
 await save();expect(onSave.mock.calls[0].slice(1,3)).toEqual(['test-shared-key','test-shared-key']);
});
it('修改手动填写的另一项后，两项密钥独立保留',async()=>{
 const onSave=await setup();await input(key('answer'),'test-shared-key');
 await input(key('jev'),'test-separate-jev');await input(key('answer'),'test-new-answer');
 expect(key('jev').value).toBe('test-separate-jev');await save();
 expect(onSave.mock.calls[0].slice(1,3)).toEqual(['test-new-answer','test-separate-jev']);
});
it('另一项已有保存的密钥时，不覆盖它',async()=>{
 const settings=structuredClone(value);settings.jev.has_key=true;
 const onSave=await setup(settings);await input(key('answer'),'test-answer-only');
 expect(key('jev').value).toBe('');await save();expect(onSave.mock.calls[0][2]).toBe('');
});
it('切换至其他供应商后，不自动共享密钥',async()=>{
 await setup();await choose('jev','official');await input(key('answer'),'test-answer-only');expect(key('jev').value).toBe('');
 await input(key('jev'),'test-typesafe-only');expect(key('answer').value).toBe('test-answer-only');
});
it('清空源密钥会清空自动填入项，切换供应商不会将密钥带到新接口',async()=>{
 await setup();await input(key('answer'),'test-shared-key');await input(key('answer'),'');expect(key('jev').value).toBe('');
 await input(key('answer'),'test-shared-key');await choose('answer','openai');expect(key('answer').value).toBe('');
 await input(key('jev'),'test-302-only');expect(key('answer').value).toBe('');
});
