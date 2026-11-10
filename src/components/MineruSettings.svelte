<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {api} from '../lib/api';
 import type {MineruConfig} from '../lib/types';
 import Input from './ui/Input.svelte';
 import Select from './ui/Select.svelte';
 import Switch from './ui/Switch.svelte';
 export let value:MineruConfig|undefined=undefined;
 export let apiKey='';
 let error='',status='',busy=false,revision=0;
 let saved:Record<string,boolean>={};
 let endpoints:Record<string,string>={};
 onMount(()=>{let active=true;
  if(value)saved={[value.provider]:value.has_key};
  Promise.resolve(api<string>('mineru_schema')).then(raw=>{if(!active)return;const schema=JSON.parse(raw);value={...schema.defaults,...value};saved=schema.keys;endpoints=schema.endpoints??{};}).catch(reason=>{if(active&&!value)error=String(reason)});
  return()=>{active=false;revision++};
 });
 function provider(nextValue:string){if(!value)return;revision++;const next=nextValue as MineruConfig['provider'];apiKey='';value={...value,provider:next,has_key:saved[next]??false};error='';status='';busy=false;}
 async function test(){if(!value)return;error='';status='';if(!apiKey.trim()&&!value.has_key){error=$t('请填写当前服务商的 API Key');return;}const current=++revision;busy=true;
  try{const result=await api<string>('test_mineru',{config:structuredClone(value),key:apiKey.trim()});if(current===revision)status=$t(result);}
  catch(reason){if(current===revision)error=$t(String(reason));}finally{if(current===revision)busy=false;}
 }
 async function clear(){if(!value)return;const config=structuredClone(value);busy=true;error='';try{await api('clear_mineru_key',{config});saved={...saved,[config.provider]:false};value={...value,has_key:false};apiKey='';status=$t('已移除保存的密钥');}catch(reason){error=String(reason)}finally{busy=false}}
</script>
<section class="settings-group settings-fields mineru-settings-card">
 <div class="settings-group-heading"><h3>{$t('文档解析 · MinerU')}</h3></div>
 {#if value}
  <p class="settings-help">{$t('保存密钥后自动优先使用 MinerU，失败后尝试本地解析。支持的文档会发送到所选服务商；测试连接不上传文件。')}</p>
  <div class="mineru-provider"><span class="field-label">{$t('服务商')}</span><Select label={$t('MinerU 服务商')} value={value.provider} onValueChange={provider} disabled={busy} items={[{value:'302',label:'302.AI'},{value:'official',label:$t('MinerU 官方')}]}/></div>
  {#if endpoints[value.provider]}<p class="mineru-endpoint"><span>Base URL</span><code>{endpoints[value.provider]}</code></p>{/if}
  <label class="field-label">{value.provider==='official'?$t('官方 API Token'):'API Key'}<Input aria-label="MinerU API Key" type="password" bind:value={apiKey} autocomplete="off" placeholder={value.has_key?$t('已保存，留空保留原密钥'):$t('填写当前服务商的 API Key')}/></label>
  <div class="mineru-test-area">
   <div class="mineru-test-result" aria-live="polite" aria-atomic="true">{#if error}<p role="alert" class="form-error">{error}</p>{:else if status}<p role="status" class="settings-help">{status}</p>{/if}</div>
  <div class="settings-section-actions"><Button.Root type="button" class="outline" onclick={test} disabled={busy}>{busy?$t('正在测试…'):$t('测试连接')}</Button.Root>{#if value.has_key}<Button.Root type="button" class="outline" onclick={clear} disabled={busy}>{$t('清除密钥')}</Button.Root>{/if}</div>
  </div>
  <details class="mineru-advanced"><summary>{$t('高级解析参数')}</summary>
   {#each [{key:'task_timeout_secs',label:'MinerU 任务时限',min:10,max:86400,unit:'秒'},{key:'request_timeout_secs',label:'MinerU 请求超时',min:5,max:3600,unit:'秒'},{key:'poll_interval_secs',label:'MinerU 查询间隔',min:1,max:60,unit:'秒'},{key:'concurrency',label:'MinerU 并行数',min:1,max:16,unit:''}] as field}
    <div class="setting-row"><label for={'mineru-'+field.key}><strong>{$t(field.label)}{field.unit?' · '+$t(field.unit):''}</strong></label><Input id={'mineru-'+field.key} aria-label={$t(field.label)} type="number" min={field.min} max={field.max} step="1" required value={Number(value[field.key as keyof MineruConfig])} oninput={event=>{if(value)value={...value,[field.key]:event.currentTarget.valueAsNumber}}}/></div>
   {/each}
   {#if value.provider==='302'}
   <label class="field-label">{$t('MinerU 版本')}<Select label={$t('MinerU 版本')} value={value.version??'2.5'} onValueChange={v=>{if(value)value={...value,version:v as '2.0'|'2.5'}}} items={[{value:'2.5',label:'2.5'},{value:'2.0',label:'2.0'}]}/></label>
   <label class="field-label">{$t('解析方式')}<Select label={$t('解析方式')} value={value.parse_method??'auto'} onValueChange={v=>{if(value)value={...value,parse_method:v as 'auto'|'txt'|'ocr'}}} items={[{value:'auto',label:$t('自动识别')},{value:'txt',label:$t('文本提取')},{value:'ocr',label:'OCR'}]}/></label>
   <p class="settings-help">{$t('开源部署版仅发送 PDF；请求超时和任务时限由客户端控制。')}</p>
   {:else}
   <label class="field-label">{$t('解析模型')}<Select label={$t('MinerU 解析模型')} bind:value={value.model_version} items={[{value:'vlm',label:'VLM'},{value:'pipeline',label:'Pipeline'}]}/></label>
   <p class="settings-help">{$t('识别选项按所选模型支持的能力执行。')}</p>
   {#each [{key:'ocr',label:'OCR 识别'},{key:'table',label:'表格识别'},{key:'formula',label:'公式识别'}] as field}<div class="setting-row"><strong>{$t(field.label)}</strong><Switch checked={Boolean(value[field.key as keyof MineruConfig])} label={'MinerU '+$t(field.label)} onCheckedChange={checked=>{if(value)value={...value,[field.key]:checked}}}/></div>{/each}
   {/if}
  </details>
 {:else if !error}<p class="settings-help">{$t('正在读取解析设置…')}</p>{/if}
 {#if !value&&error}<p role="alert" class="form-error">{error}</p>{/if}
</section>

<style>
 .mineru-settings-card{display:flex;flex-direction:column;gap:18px}
 .mineru-settings-card .settings-help{margin:0;line-height:1.6}
 .mineru-test-area{display:flex;align-items:center;gap:16px}
 .mineru-test-result{flex:1;min-width:0;overflow-wrap:anywhere}
 .mineru-test-area .settings-section-actions{flex-shrink:0;margin:0}
 .mineru-endpoint{display:flex;flex-wrap:wrap;gap:8px;color:var(--color-muted);font-size:12px;margin:0}
 .mineru-endpoint code{overflow-wrap:anywhere;user-select:text}
 .mineru-provider{display:grid;grid-template-columns:1fr minmax(180px,240px);align-items:center;gap:16px}
 .field-label{display:flex;flex-direction:column;gap:8px}
 .mineru-advanced{border-top:1px solid var(--color-rule);padding-top:16px}
 summary{cursor:pointer;font-weight:500;padding:4px 0}
 .mineru-advanced .setting-row{padding:14px 0;gap:24px}
 .mineru-advanced :global(input[type=number]){width:120px;flex-shrink:0}
 .mineru-advanced>.field-label{margin:16px 0}
</style>
