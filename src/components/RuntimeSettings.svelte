<script lang="ts">
 import {onMount} from 'svelte';
 import {Button} from 'bits-ui';
 import {api} from '../lib/api';
 import {t} from '../lib/i18n';
 import Input from './ui/Input.svelte';
 import Switch from './ui/Switch.svelte';
 export let value:Record<string,number|boolean>={};
 export let group:string;
 type Field={key:string;label:string;help:string;unit:string;min:number;max:number;group:string;advanced:boolean};
 let resources:number[]|null=null;
 let fields:Field[]=[],defaults:Record<string,number|boolean>={},error='',loading=true;
 onMount(()=>{let active=true;Promise.resolve(api<string>('runtime_schema')).then(raw=>{
   if(!active)return;const schema=typeof raw==='string'?JSON.parse(raw):raw;
   if(!schema?.defaults||!Array.isArray(schema.fields))throw new Error($t('无法读取运行参数'));
   defaults=schema.defaults;fields=schema.fields;resources=schema.resources??null;value={...defaults,...value};
  }).catch(reason=>{if(active)error=String(reason)}).finally(()=>{if(active)loading=false});return()=>{active=false}});
 $:visible=fields.filter(field=>field.group===group);
 function reset(){value={...value,...Object.fromEntries(visible.map(field=>[field.key,defaults[field.key]]))}}
 function set(key:string,event:Event){const input=event.currentTarget as HTMLInputElement;value={...value,[key]:input.valueAsNumber}}
</script>
{#if loading}<p class="settings-help">{$t('正在读取运行参数…')}</p>
{:else if error}<p class="form-error" role="alert">{error}</p>
{:else if visible.length}
 {#each visible.filter(field=>!field.advanced) as field}
  <section class="settings-group"><div class="setting-row"><label for={'runtime-'+field.key}><strong>{$t(field.label)}{field.unit?' · '+$t(field.unit):''}</strong><p>{$t(field.help)}</p></label><Input id={'runtime-'+field.key} aria-label={$t(field.label)} type="number" min={field.min} max={field.max} step="1" required value={Number(value[field.key])} oninput={event=>set(field.key,event)}/></div></section>
 {/each}
 {#if group==='files'&&resources}<p class="settings-help">{$t('当前本地转换内存预算 {value0} MiB，单任务 {value1} MiB，最多同时运行 {value2} 个任务。',{value0:Math.floor(resources[0]/1048576),value1:Math.floor(resources[1]/1048576),value2:resources[2]})}</p>{/if}
 <details class="runtime-settings"><summary>{$t('高级运行参数')}</summary><p class="settings-help">{$t('保存后新任务生效，运行中的任务继续使用原参数。资源不足时会等待可用内存。')}</p>
  <section class="settings-group">
   {#each visible.filter(field=>field.advanced) as field}
    <div class="setting-row"><label for={'runtime-'+field.key}><strong>{$t(field.label)}{field.unit?' · '+$t(field.unit):''}</strong><p>{$t(field.help)}</p></label>
    {#if typeof defaults[field.key]==='boolean'}<Switch checked={Boolean(value[field.key])} onCheckedChange={checked=>value={...value,[field.key]:checked}} label={$t(field.label)}/>
    {:else}<Input id={'runtime-'+field.key} aria-label={$t(field.label)} type="number" min={field.min} max={field.max} step="1" required value={Number(value[field.key])} oninput={event=>set(field.key,event)}/>{/if}</div>
   {/each}
  </section>
 </details>
 <div class="settings-section-actions"><Button.Root type="button" class="outline" onclick={reset}>{$t('恢复本组默认')}</Button.Root></div>
{/if}
