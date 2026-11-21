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
const authTrigger=()=>card('jev').querySelector<HTMLButtonElement>('.auth-field button')!;
async function input(element:HTMLInputElement|HTMLTextAreaElement,text:string){element.value=text;element.dispatchEvent(new Event('input',{bubbles:true}));await tick();}
async function choose(kind:ModelKind,id:string){
 trigger(kind).dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector(`[role=option][data-value="${id}"]`)).not.toBeNull());
 document.querySelector(`[role=option][data-value="${id}"]`)!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));
 await tick();
}
async function chooseAuth(id:'bearer'|'none'){
 expect(authTrigger()).not.toBeNull();
 authTrigger().dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector(`[role=option][data-value="${id}"]`)).not.toBeNull());
 document.querySelector(`[role=option][data-value="${id}"]`)!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));
 await tick();
}
async function setup(settings=value,initial='models'){const onSave=vi.fn().mockResolvedValue(undefined);app=mount(SettingsPanel,{target:document.body,props:{value:structuredClone(settings),initial,onClose:()=>{},onSave}});await tick();return onSave;}
async function save(){document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));await tick();}
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML='';api.mockReset();});

it('在解析模型配置中分别编辑和恢复 PDF 与图片提示词',async()=>{
 api.mockImplementation(async command=>command==='runtime_schema'?JSON.stringify({defaults:{},fields:[]}):[]);
 const onSave=await setup({...value,document_parser:'vision',document_parser_model:'second'},'files');
 [...document.querySelectorAll('button')].find(button=>button.textContent?.trim()==='配置模型')!.click();
 const dialog=()=>document.querySelector<HTMLElement>('[role=dialog][aria-label="配置模型"]')!;
 const pdf=()=>dialog().querySelector<HTMLTextAreaElement>('#vision-pdf-prompt')!;
 const image=()=>dialog().querySelector<HTMLTextAreaElement>('#vision-image-prompt')!;
 await vi.waitFor(()=>expect(pdf().value).toContain('Transcribe this PDF page into Markdown'));
 expect(image().value).toContain('Transcribe visible text in reading order');
 const defaults={pdf:pdf().value,image:image().value};
 const edit=async(element:HTMLTextAreaElement,text:string)=>{
  element.value=text;element.dispatchEvent(new Event('input',{bubbles:true}));await tick();
 };
 await edit(pdf(),'自定义 PDF 提示词');
 await edit(image(),'自定义图片提示词');
 dialog().querySelector<HTMLButtonElement>('[aria-describedby="vision-pdf-label"]')!.click();
 await tick();
 expect(pdf().value).toBe(defaults.pdf);
 expect(image().value).toBe('自定义图片提示词');
 dialog().querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await tick();
 expect(onSave).not.toHaveBeenCalled();
 await save();
 expect(onSave.mock.calls[0][0]).toMatchObject({document_parser_pdf_prompt:'',document_parser_image_prompt:'自定义图片提示词'});
});

it('文件设置按所选解析器打开配置，切回 Docling 后保留 MinerU 密钥',async()=>{
 const mineru={enabled:true,provider:'302' as const,has_key:true,request_timeout_secs:60,task_timeout_secs:1800,poll_interval_secs:2,concurrency:2,model_version:'vlm' as const,ocr:true,table:true,formula:true};
 api.mockImplementation(async command=>{
  if(command==='mineru_schema')return JSON.stringify({defaults:mineru,keys:{'302':true},endpoints:{'302':'https://api.302ai.cn'}});
  if(command==='runtime_schema')return JSON.stringify({defaults:{},fields:[]});
  if(command==='get_docling_status')return {installed:false,running:false,formats:[]};
  return [];
 });
 const onSave=await setup({...value,mineru},'files');
 const button=(name:string,root:ParentNode=document)=>[...root.querySelectorAll('button')].find(button=>button.textContent?.trim()===name)!;
 const parser=document.querySelector<HTMLButtonElement>('[aria-label="优先解析方式"]')!;
 expect(parser.textContent).toContain('MinerU');
 button('配置 MinerU').click();
 await vi.waitFor(()=>expect(document.querySelector('.mineru-config-dialog [aria-label="MinerU API Key"]')).not.toBeNull());
 button('完成',document.querySelector('.mineru-config-dialog')!).click();
 await tick();
 parser.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role=option][data-value="docling"]')).not.toBeNull());
 document.querySelector('[role=option][data-value="docling"]')!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));
 await tick();
 button('配置 Docling').click();
 await vi.waitFor(()=>expect(document.querySelector('[role=dialog][aria-label="配置 Docling"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[aria-label="关闭格式选择"]')!.click();
 await tick();
 await save();
 expect(onSave.mock.calls[0][0]).toMatchObject({document_parser:'docling',mineru:{has_key:true}});
 expect(api).not.toHaveBeenCalledWith('clear_mineru_key',expect.anything());
});

it('已有 302 配置默认显示 302，打开设置不覆盖地址',async()=>{
 await setup();expect(trigger('answer').textContent).toContain('302.AI');expect(trigger('jev').textContent).toContain('302.AI');
 expect(address('answer').value).toBe(value.answer.url);expect(address('jev').value).toBe(value.jev.url);expect(api.mock.calls.filter(([command])=>!['mineru_schema','runtime_schema'].includes(command))).toHaveLength(0);
});

it('打开所选解析模型的配置，在弹窗内测试连接并保存共享配置',async()=>{
 api.mockImplementation(async command=>{
  if(command==='runtime_schema')return JSON.stringify({defaults:{},fields:[]});
  if(command==='get_docling_status')return {installed:false,running:false,formats:[]};
  if(command==='test_model')return '连接成功';
  return [];
 });
 const onSave=await setup({...value,answer:{...value.answer,has_key:true}},'files');
 const parser=document.querySelector<HTMLButtonElement>('[aria-label="优先解析方式"]')!;
 parser.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role=option][data-value="model:second"]')).not.toBeNull());
 document.querySelector('[role=option][data-value="model:second"]')!.dispatchEvent(new PointerEvent('pointerup',{bubbles:true,pointerType:'mouse'}));
 await tick();
 expect(parser.textContent).toContain('second');
 expect(document.body.textContent).toContain('将使用所选模型的多模态能力解析 PDF 和图片。');
 [...document.querySelectorAll('button')].find(button=>button.textContent?.trim()==='配置模型')!.click();
 const dialog=()=>document.querySelector<HTMLElement>('[role=dialog][aria-label="配置模型"]')!;
 await vi.waitFor(()=>expect(dialog()?.querySelector('h2')?.textContent).toBe('second'));
 expect(api.mock.calls.filter(([command])=>command==='test_model')).toHaveLength(0);
 const endpoint=dialog().querySelector<HTMLInputElement>('input[type=url]')!;
 expect(endpoint.value).toBe(value.answer.url);
 const url='https://api.302.ai/team/v1';
 await input(endpoint,url);
 [...dialog().querySelectorAll('button')].find(button=>button.textContent?.trim()==='测试连接')!.click();
 await vi.waitFor(()=>expect(dialog().querySelector('[role=status]')?.textContent).toBe('连接成功'));
 expect(api).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:'vision',config:expect.objectContaining({model:'second',url})}));
 dialog().querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await tick();
 await save();
 expect(onSave.mock.calls[0][0]).toMatchObject({document_parser:'vision',document_parser_model:'second',answer:{model:'first',url}});
});
it('302 密钥入口打开指定网页，切换供应商后隐藏',async()=>{
 await setup();
 const link=()=>card('answer').querySelector<HTMLAnchorElement>('a.settings-key-link');
 expect(link()?.textContent).toBe('获取 API Key');
 expect(link()?.href).toBe('https://302.ai/api-keys/list');
 link()!.click();await tick();
 expect(api).toHaveBeenCalledWith('open_about_link',{target:'302-api-keys'});
 await choose('answer','openai');expect(link()).toBeNull();
 expect(card('jev').querySelector('.settings-key-link')).not.toBeNull();
});
it('获取问答模型列表后可筛选并追加，保留已有模型直到保存',async()=>{
 const answer={...value.answer,has_key:true};
 const onSave=await setup({...value,answer});
 api.mockResolvedValue(['first','demo-fast','demo-reasoning']);
 const button=card('answer').querySelector<HTMLButtonElement>('button.settings-key-link')!;
 button.click();
 const dialog=()=>document.querySelector<HTMLElement>('[role=dialog][aria-label="模型列表"]')!;
 await vi.waitFor(()=>expect(dialog()?.querySelectorAll('input[type=checkbox]')).toHaveLength(3));
 expect(api).toHaveBeenCalledWith('list_answer_models',{config:answer,key:'',modelProxy:{mode:'system',url:'',username:'',bypass:'',has_password:false,password:null}});
 expect(dialog().querySelector<HTMLInputElement>('input[type=checkbox]')?.disabled).toBe(true);
 await input(dialog().querySelector<HTMLInputElement>('input[type=search]')!,'FAST');
 const option=dialog().querySelector<HTMLInputElement>('input[type=checkbox]')!;
 option.click();await tick();
 expect(card('answer').querySelector('textarea')?.value).toBe('first\nsecond\ndemo-fast');
 expect(onSave).not.toHaveBeenCalled();
 option.click();await tick();
 expect(card('answer').querySelector('textarea')?.value).toBe('first\nsecond');
 option.click();await tick();
 Array.from(dialog().querySelectorAll('button')).find(button=>button.textContent==='完成')!.click();
 await tick();await save();
 expect(onSave.mock.calls[0][0].answer.models).toEqual(['first','second','demo-fast']);
});
it('LLM 六个供应商自动填入地址，自定义地址与模型列表可以保存',async()=>{
 const onSave=await setup();
 for(const provider of providers.answer.filter(provider=>provider.value!=='custom')){await choose('answer',provider.value);expect(address('answer').value).toBe(provider.value==='302'?value.answer.url:provider.url);}
 await choose('answer','custom');await input(address('answer'),'https://proxy.example/v1');await save();
 expect(onSave.mock.calls[0][0].answer).toMatchObject({url:'https://proxy.example/v1',model:'first',models:['first','second']});
});
it('Jev 预设供应商填入对应评分接口，并提供自定义选项',async()=>{
 expect(providers.jev.map(provider=>provider.label)).toEqual(['Zleap','OpenAI','302.AI','OpenRouter','Typesafe','自定义']);
 await setup();
 for(const provider of providers.jev.filter(provider=>provider.value!=='custom')){await choose('jev',provider.value);expect(address('jev').value).toBe(provider.url);}
 trigger('jev').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true}));
 await vi.waitFor(()=>expect(document.querySelectorAll('[role=option]')).toHaveLength(6));
 expect(document.querySelector('[role=option][data-value=custom]')).not.toBeNull();
});
it('切换到 OpenAI 决策模型后使用官方接口和新密钥',async()=>{
 const onSave=await setup({...value,jev:{...value.jev,url:'http://127.0.0.1:8000/v1',auth_mode:'none'}});
 await choose('jev','openai');
 expect(address('jev').value).toBe('https://api.openai.com/v1/decisions');
 await input(key('jev'),'test-decisions-key');api.mockResolvedValue('连接成功');
 card('jev').querySelector<HTMLButtonElement>('.settings-model-test')!.click();
 const model={url:'https://api.openai.com/v1/decisions',model:'gpt-6-luna',auth_mode:'bearer'};
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:'jev',key:'test-decisions-key',config:expect.objectContaining(model)})));
 await save();expect(onSave.mock.calls[0][0].jev).toMatchObject(model);
 expect(onSave.mock.calls[0][0].answer).toEqual(value.answer);
});
it('Jev 自定义地址用于测试连接，保存后重新打开仍显示该地址',async()=>{
 const onSave=await setup();await choose('jev','custom');
 await input(address('jev'),'https://scorer.example/v1/decisions');
 expect(trigger('jev').textContent).toContain('自定义');
 await input(key('jev'),'test-custom-key');api.mockResolvedValue('连接成功');
 card('jev').querySelector<HTMLButtonElement>('.settings-model-test')!.click();
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
 card('answer').querySelector<HTMLButtonElement>('.settings-model-test')!.click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('切换供应商后，请填写对应的 API 密钥。'));expect(api.mock.calls.filter(([command])=>!['mineru_schema','runtime_schema'].includes(command))).toHaveLength(0);
 await save();expect(onSave).not.toHaveBeenCalled();
 await input(key('answer'),'test-openai-key');await choose('answer','302');expect(key('answer').value).toBe('');
 await choose('answer','openai');await input(key('answer'),'test-new-key');api.mockResolvedValue('连接成功');
 card('answer').querySelector<HTMLButtonElement>('.settings-model-test')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:'answer',key:'test-new-key',config:expect.objectContaining({url:'https://api.openai.com/v1'})})));
 await save();expect(onSave.mock.calls[0][1]).toBe('test-new-key');
});
it('网络代理页的修改随模型测试和设置保存生效',async()=>{
 api.mockImplementation(async command=>command==='runtime_schema'?{defaults:{},fields:[]}:'连接成功');
 const proxy={mode:'custom' as const,url:'http://gateway.example:8080',username:'',bypass:'',has_password:false,password:null};
 const onSave=await setup({...value,model_proxy:proxy},'general');
 const nav=document.querySelector('.settings-nav')!;
 const openPage=(name:string)=>[...nav.querySelectorAll('button')].find(button=>button.textContent?.trim()===name)!.click();
 expect(document.querySelector('.settings-content')?.textContent).not.toContain('配置代理');
 openPage('网络代理');await tick();
 const modelProxy={...proxy,url:'socks5://gateway.example:1080',bypass:'.office.example, 10.0.0.0/8'};
 await input(document.querySelector<HTMLInputElement>('#network-proxy-url')!,modelProxy.url);
 await input(document.querySelector<HTMLTextAreaElement>('#network-proxy-bypass')!,modelProxy.bypass);
 openPage('模型设置');await tick();
 card('jev').querySelector<HTMLButtonElement>('.settings-model-test')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:'jev',modelProxy})));
 expect(onSave).not.toHaveBeenCalled();
 await save();expect(onSave.mock.calls[0][0].model_proxy).toEqual(modelProxy);
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
 api.mockResolvedValue('连接成功');card(other).querySelector<HTMLButtonElement>('.settings-model-test')!.click();
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

it('旧 Jev 配置默认使用 API Key，问答模型不出现鉴权选项',async()=>{
 await setup();
 expect(authTrigger()).not.toBeNull();expect(authTrigger().textContent).toContain('API Key');
 expect(card('answer').querySelector('.auth-field')).toBeNull();
 expect(key('jev')).not.toBeNull();
});
it('本地 Jev 无需鉴权时允许测试和保存，重新打开后保持该模式',async()=>{
 const settings=structuredClone(value);settings.jev.has_key=true;
 const onSave=await setup(settings);
 await choose('jev','custom');await input(address('jev'),'http://127.0.0.1:8000/v1/systemone');
 await chooseAuth('none');
 expect(key('jev')).toBeNull();api.mockResolvedValue('连接成功');
 card('jev').querySelector<HTMLButtonElement>('.settings-model-test')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:'jev',key:'',config:expect.objectContaining({auth_mode:'none',url:'http://127.0.0.1:8000/v1/systemone'})})));
 await vi.waitFor(()=>expect(document.body.textContent).toContain('连接成功'));
 await save();expect(onSave).toHaveBeenCalledTimes(1);
 expect(onSave.mock.calls[0][0].jev.auth_mode).toBe('none');expect(onSave.mock.calls[0][2]).toBe('');
 const saved=structuredClone(onSave.mock.calls[0][0]);
 await unmount(app!);app=undefined;document.body.innerHTML='';await setup(saved);
 expect(authTrigger().textContent).toContain('无需鉴权');expect(key('jev')).toBeNull();
});
it('无需鉴权会丢弃草稿密钥，也不接收 302 自动共享的密钥',async()=>{
 const onSave=await setup();await input(key('jev'),'test-draft-jev');
 await chooseAuth('none');await input(key('answer'),'test-answer-updated');
 api.mockResolvedValue('连接成功');card('jev').querySelector<HTMLButtonElement>('.settings-model-test')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:'jev',key:''})));
 await save();expect(onSave.mock.calls[0][2]).toBe('');
 await chooseAuth('bearer');expect(key('jev').value).toBe('');
});
it('切回 API Key 后仍禁止复用原供应商的保存密钥',async()=>{
 const settings=structuredClone(value);settings.jev.has_key=true;const onSave=await setup(settings);
 await chooseAuth('none');await choose('jev','custom');await input(address('jev'),'http://127.0.0.1:8000/v1');
 await chooseAuth('bearer');
 expect(key('jev').placeholder).toBe('填写 API 密钥');
 card('jev').querySelector<HTMLButtonElement>('.settings-model-test')!.click();
 await vi.waitFor(()=>expect(document.body.textContent).toContain('切换供应商后，请填写对应的 API 密钥。'));
 expect(api.mock.calls.filter(([command])=>!['mineru_schema','runtime_schema'].includes(command))).toHaveLength(0);await save();expect(onSave).not.toHaveBeenCalled();
});

it('切换到 Zleap 自动填写 Zev-1，测试和保存使用新模型',async()=>{
 const onSave=await setup();
 const model=card('jev').querySelector<HTMLInputElement>('input[type=text]')!;
 expect(model.value).toBe('jev-latest');
 await choose('jev','zleap');
 expect(model.value).toBe('Zev-1');expect(address('jev').value).toBe('https://api.zleap.com/v1');
 await input(key('jev'),'test-zleap-key');api.mockResolvedValue('连接成功');
 card('jev').querySelector<HTMLButtonElement>('.settings-model-test')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('test_model',expect.objectContaining({kind:'jev',key:'test-zleap-key',config:expect.objectContaining({model:'Zev-1'})})));
 await save();expect(onSave).toHaveBeenCalledTimes(1);
 expect(onSave.mock.calls[0][0].jev.model).toBe('Zev-1');
});
it('打开设置保留已有 Zleap 模型，切走后切回使用 Zev-1',async()=>{
 const settings=structuredClone(value);settings.jev={...settings.jev,url:'https://api.zleap.com/v1',model:'custom-decision'};
 await setup(settings);
 const model=card('jev').querySelector<HTMLInputElement>('input[type=text]')!;
 expect(model.value).toBe('custom-decision');
 await choose('jev','302');expect(model.value).toBe('jev-latest');
 await choose('jev','zleap');expect(model.value).toBe('Zev-1');
});
it('Zleap 预设显示控制台密钥入口，并与 302 问答密钥隔离',async()=>{
 const settings=structuredClone(value);settings.jev={...settings.jev,url:'https://api.zleap.com/v1',model:'Zev-1',auth_mode:'bearer'};
 const onSave=await setup(settings);
 expect(trigger('jev').textContent).toContain('Zleap');
 const link=card('jev').querySelector<HTMLAnchorElement>('.settings-key-link')!;
 expect(link.href).toBe('https://platform.zleap.com/console');
 link.click();await tick();expect(api).toHaveBeenCalledWith('open_about_link',{target:'zleap-console'});
 await input(key('answer'),'test-answer-key');expect(key('jev').value).toBe('');
 await input(key('jev'),'test-zleap-key');await save();
 expect(onSave.mock.calls[0].slice(1,3)).toEqual(['test-answer-key','test-zleap-key']);
 await choose('jev','official');expect(card('jev').querySelector('.settings-key-link')).toBeNull();
});

it('决策供应商切换同步默认模型，其他供应商清空，允许手动填写',async()=>{
 const onSave=await setup();
 const model=card('jev').querySelector<HTMLInputElement>('input[type=text]')!;
 for(const [provider,expected] of [['zleap','Zev-1'],['302','jev-latest'],['openrouter',''],['official','jev-latest'],['custom','']]){
  await choose('jev',provider);expect(model.value).toBe(expected);
 }
 await input(model,'my-decision-model');
 await input(address('jev'),'http://127.0.0.1:8000/v1');
 await save();expect(onSave).toHaveBeenCalledTimes(1);
 expect(onSave.mock.calls[0][0].jev.model).toBe('my-decision-model');
 expect(onSave.mock.calls[0][0].answer.model).toBe(value.answer.model);
});
