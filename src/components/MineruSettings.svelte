<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {api} from '../lib/api';
 import type {MineruConfig} from '../lib/types';
 import Input from './ui/Input.svelte';
 import Switch from './ui/Switch.svelte';
 export let value:MineruConfig|undefined=undefined;
 export let apiKey='';
 let error='',status='',busy=false,revision=0;
 let saved:Record<string,boolean>={};
 onMount(()=>{let active=true;
  if(value)saved={[value.provider]:value.has_key};
  Promise.resolve(api<string>('mineru_schema')).then(raw=>{if(!active)return;const schema=JSON.parse(raw);value={...schema.defaults,...value};saved=schema.keys;}).catch(reason=>{if(active&&!value)error=String(reason)});
  return()=>{active=false;revision++};
 });
 function provider(event:Event){if(!value)return;revision++;const next=(event.currentTarget as HTMLSelectElement).value as MineruConfig['provider'];apiKey='';value={...value,provider:next,has_key:saved[next]??false};error='';status='';busy=false;}
 async function test(){if(!value)return;error='';status='';if(!apiKey.trim()&&!value.has_key){error=$t('请填写当前服务商的 API Key');return;}const current=++revision;busy=true;
  try{const result=await api<string>('test_mineru',{config:structuredClone(value),key:apiKey});if(current===revision)status=$t(result);}
  catch(reason){if(current===revision)error=$t(String(reason));}finally{if(current===revision)busy=false;}
 }
 async function clear(){if(!value)return;const config=structuredClone(value);busy=true;error='';try{await api('clear_mineru_key',{config});saved={...saved,[config.provider]:false};value={...value,has_key:false};apiKey='';status=$t('已移除保存的密钥');}catch(reason){error=String(reason)}finally{busy=false}}
</script>
<section class="settings-group settings-fields mineru-settings-card">
 <div class="settings-group-heading"><h3>{$t('文档解析 · MinerU')}</h3></div>
 {#if value}
  <div class="setting-row"><div><strong>{$t('启用 MinerU')}</strong><p>{$t('优先使用 MinerU，失败后依次尝试内置解析和本地 Docling。')}</p></div><Switch bind:checked={value.enabled} label={$t('启用 MinerU')}/></div>
  <p class="settings-help">{$t('启用后，支持的文档会发送到所选服务商。测试连接不上传文件。')}</p>
  <label class="field-label">{$t('服务商')}<select class="ui-input" aria-label={$t('MinerU 服务商')} value={value.provider} onchange={provider} disabled={busy}><option value="302">302.AI</option><option value="official">{$t('MinerU 官方')}</option></select></label>
  <label class="field-label">{value.provider==='official'?$t('官方 API Token'):'API Key'}<Input aria-label="MinerU API Key" type="password" bind:value={apiKey} autocomplete="off" placeholder={value.has_key?$t('已保存，留空保留原密钥'):$t('填写当前服务商的 API Key')}/></label>
  <div class="settings-section-actions"><Button.Root type="button" class="outline" onclick={test} disabled={busy}>{busy?$t('正在测试…'):$t('测试连接')}</Button.Root>{#if value.has_key}<Button.Root type="button" class="outline" onclick={clear} disabled={busy}>{$t('清除密钥')}</Button.Root>{/if}</div>
  <details><summary>{$t('高级解析参数')}</summary>
   {#each [{key:'task_timeout_secs',label:'MinerU 任务时限',min:10,max:86400,unit:'秒'},{key:'request_timeout_secs',label:'MinerU 请求超时',min:5,max:3600,unit:'秒'},{key:'poll_interval_secs',label:'MinerU 查询间隔',min:1,max:60,unit:'秒'},{key:'concurrency',label:'MinerU 并行数',min:1,max:16,unit:''}] as field}
    <div class="setting-row"><label for={'mineru-'+field.key}><strong>{$t(field.label)}{field.unit?' · '+$t(field.unit):''}</strong></label><Input id={'mineru-'+field.key} aria-label={$t(field.label)} type="number" min={field.min} max={field.max} step="1" required value={Number(value[field.key as keyof MineruConfig])} oninput={event=>{if(value)value={...value,[field.key]:event.currentTarget.valueAsNumber}}}/></div>
   {/each}
   <label class="field-label">{$t('解析模型')}<select class="ui-input" aria-label={$t('MinerU 解析模型')} bind:value={value.model_version}><option value="vlm">VLM</option><option value="pipeline">Pipeline</option></select></label>
   <p class="settings-help">{$t('识别选项按所选模型支持的能力执行。')}</p>
   {#each [{key:'ocr',label:'OCR 识别'},{key:'table',label:'表格识别'},{key:'formula',label:'公式识别'}] as field}<div class="setting-row"><strong>{$t(field.label)}</strong><Switch checked={Boolean(value[field.key as keyof MineruConfig])} label={'MinerU '+$t(field.label)} onCheckedChange={checked=>{if(value)value={...value,[field.key]:checked}}}/></div>{/each}
  </details>
 {:else if !error}<p class="settings-help">{$t('正在读取解析设置…')}</p>{/if}
 {#if error}<p role="alert" class="form-error">{error}</p>{/if}
 {#if status}<p role="status" class="settings-help">{status}</p>{/if}
</section>
