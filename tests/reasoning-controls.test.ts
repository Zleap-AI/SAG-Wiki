// @vitest-environment happy-dom
import {afterEach, expect, it, vi} from 'vitest';
import {mount, tick, unmount} from 'svelte';
import SettingsPanel from '../src/components/Settings.svelte';
import ContextUsage from '../src/components/ContextUsage.svelte';
import ReasoningControl from '../src/components/ReasoningControl.svelte';
import {reasoningLevels} from '../src/lib/reasoning';
import type {Settings} from '../src/lib/types';

let app:ReturnType<typeof mount>;
afterEach(async()=>{if(app)await unmount(app);document.body.innerHTML=''});

it('不同服务与模型只提供已配置的思考档位',()=>{
 const endpoint='https://api.302ai.cn/v1';
 expect(reasoningLevels('qwen3.6-flash',endpoint).map(level=>level.value)).toEqual([null,'none','low','medium','high','max']);
 expect(reasoningLevels('deepseek-flash',endpoint).map(level=>level.value)).toEqual([null,'none','low','high','max']);
 expect(reasoningLevels('deepseek-flash','https://other.example/v1')).toEqual([{value:null,label:'默认'}]);
 expect(reasoningLevels('unknown',endpoint)).toEqual([{value:null,label:'默认'}]);
});

it('滑块用键盘选择强度，按模型保存并支持恢复默认',async()=>{
 const value:Settings={answer:{url:'https://api.302ai.cn/v1',model:'qwen3.6-flash',models:['qwen3.6-flash','deepseek-flash'],has_key:false},jev:{url:'http://localhost',model:'jev',models:[],has_key:false},retrieval:{candidate_k:50,seed_k:10,expansion_term_limit:20,max_hops:1,top_k:10,chunk_chars:2200},theme:'light',start_page:'ask',editor_size:15,show_hidden:true,watch:true,excluded:[],timeout_secs:120,answer_context_tokens:65536,answer_output_tokens:2048,model_profiles:{'deepseek-flash':{system_prompt:'保留',sag2_enabled:false,mcp_servers:[],mcp_tools:{},reasoning_effort:'low'}}};
 const onSave=vi.fn(async(_settings:Settings)=>{});
 app=mount(SettingsPanel,{target:document.body,props:{value,initial:'answer',onSave,onClose:()=>{}}});
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="思考强度"]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[aria-label="思考强度"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[role="slider"]')).not.toBeNull());
 const slider=document.querySelector<HTMLElement>('[role="slider"]')!;
 slider.dispatchEvent(new KeyboardEvent('keydown',{key:'End',bubbles:true,cancelable:true}));
 await tick();
 expect(slider.getAttribute('aria-valuetext')).toBe('极高');
 document.querySelector('form')!.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));
 await tick();
 expect(onSave.mock.calls[0][0].model_profiles!['qwen3.6-flash'].reasoning_effort).toBe('max');
 expect(onSave.mock.calls[0][0].model_profiles!['deepseek-flash']).toEqual(value.model_profiles!['deepseek-flash']);
 document.querySelector<HTMLButtonElement>('[aria-label="恢复默认思考强度"]')!.click();
 await tick();
 expect(slider.getAttribute('aria-valuetext')).toBe('默认');
});

it('上下文圆环使用最近一轮输入估算，不累计多次工具调用的用量',async()=>{
 app=mount(ContextUsage,{target:document.body,props:{limit:65536,context:{limit:65536,reserved_output:2048,estimated_input:16384,omitted_sources:0,omitted_history:0,input_tokens:200000,output_tokens:1000}}});
 await tick();
 const ring=document.querySelector<HTMLButtonElement>('[aria-label="上下文占用约 25%"]')!;
 expect(ring).not.toBeNull();
 ring.click();
 await vi.waitFor(()=>expect(document.querySelector('.context-popover')).not.toBeNull());
 expect(document.querySelector('.context-popover')!.textContent).toContain('16.4K / 65.5K');
});

it('模型和强度共用入口，展开后可以切换模型',async()=>{
 const onModelChange=vi.fn();
 app=mount(ReasoningControl,{target:document.body,props:{model:'deepseek-flash',models:['deepseek-flash','qwen3.6-flash'],endpoint:'https://api.302ai.cn/v1',value:'high',onChange:()=>{},onModelChange}});
 await tick();
 const trigger=document.querySelector<HTMLButtonElement>('[aria-label="模型与思考强度"]')!;
 expect(trigger.textContent).toContain('deepseek-flash');
 expect(trigger.textContent).toContain('高');
 expect(trigger.querySelectorAll('svg').length).toBe(1);
 trigger.click();
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="选择问答模型"]')).not.toBeNull());
 const modelTrigger=document.querySelector<HTMLButtonElement>('[aria-label="选择问答模型"]')!;
 modelTrigger.focus();
 modelTrigger.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('[role=option][data-value="qwen3.6-flash"]')).not.toBeNull());
 expect(document.querySelector('[role=slider]')).toBeNull();
 (document.querySelector('[role=option][data-value="qwen3.6-flash"]')! as HTMLElement).click();
 await vi.waitFor(()=>expect(onModelChange).toHaveBeenCalledWith('qwen3.6-flash'));
 expect(document.querySelector('[role=slider]')).toBeNull();
 expect(document.querySelector('[aria-label="模型列表"]')).toBeNull();
 trigger.click();
 await vi.waitFor(()=>expect(document.querySelector('[role=slider]')).not.toBeNull());
 document.querySelector<HTMLButtonElement>('[aria-label="选择问答模型"]')!.click();
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="模型列表"]')).not.toBeNull());
 document.querySelector('.reasoning-model-command')!.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
 await vi.waitFor(()=>expect(document.querySelector('[aria-label="模型列表"]')).toBeNull());
});
