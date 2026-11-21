<script lang="ts">
 import {onDestroy} from 'svelte';
 import {Button} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {api} from '../lib/api';
 import {icon} from '../lib/icons';
 import type {McpCredentials,Settings} from '../lib/types';
 import {jevConfigured,quickAnswerConfig,quickJevConfig,quickMineruConfig,quickSettings,quickSetup} from '../lib/quick-setup';
 import Modal from './ui/Modal.svelte';
 import Input from './ui/Input.svelte';
 export let settings:Settings;
 export let onClose:()=>void;
 export let onSave:(settings:Settings,answer:string,jev:string,credentials?:Record<string,McpCredentials>,mineruKey?:string)=>Promise<void>;
 export let onManual:(()=>void)|undefined=undefined;
 export let onSaved:(()=>void)|undefined=undefined;
 let apiKey='',showKey=false,busy=false,error='',status='';
 // Services that already work are left alone; only the missing ones are tested and written.
 // MinerU counts as working only when parsing is routed to it, not merely when a key is stored.
 const mineruSaved=settings.mineru?.enabled===true;
 const jevSaved=jevConfigured(settings.jev);
 const mineru=!mineruSaved;
 let destroyed=false;
 onDestroy(()=>{destroyed=true});
 async function openKeyLink(){error='';try{await api('open_about_link',{target:'302-api-keys'})}catch(e){error=$t(String(e))}}
 async function run(){
  const key=apiKey.trim();
  if(!key){error=$t('请填写 302.AI 的 API Key');return;}
  busy=true;error='';
  try{
   const modelProxy=settings.model_proxy?structuredClone(settings.model_proxy):undefined;
   status=$t('正在测试问答模型…');
   await api('test_model',{kind:'answer',config:quickAnswerConfig(settings.answer),key,modelProxy});
   if(!jevSaved){status=$t('正在测试 Jev 决策模型…');await api('test_model',{kind:'jev',config:quickJevConfig(settings.jev),key,modelProxy});}
   if(mineru){status=$t('正在测试 MinerU…');await api('test_mineru',{config:quickMineruConfig(settings.mineru),key,modelProxy});}
   // The dialog may have been closed while a test was running; nothing is written then.
   if(destroyed)return;
   status=$t('正在保存…');
   await onSave(quickSettings(settings,mineru),key,jevSaved?'':key,{},mineru?key:'');
   onSaved?.();onClose();
  }catch(e){error=$t(String(e));}
  finally{busy=false;status='';}
 }
</script>

<Modal label={$t('快速配置')} class="quick-setup-dialog" onClose={()=>{if(!busy)onClose()}}>
 <div class="dialog-head quick-setup-head"><h2>{$t('快速配置')}</h2><Button.Root type="button" class="icon-button" aria-label={$t('关闭')} onclick={onClose} disabled={busy}>{@html icon('close')}</Button.Root></div>
 <form class="dialog-body quick-setup-body" on:submit|preventDefault={run}>
  <p class="quick-setup-intro">{$t('填入 302.AI 的 API Key，自动配置问答模型、Jev 决策模型和 MinerU 文档解析。')}</p>
  <div class="quick-setup-field">
   <div class="quick-setup-label"><span>API Key</span><Button.Root type="button" class="quick-setup-link" onclick={openKeyLink} disabled={busy}>{$t('获取 API Key')}{@html icon('externalLink')}</Button.Root></div>
   <div class="quick-setup-key" class:invalid={!!error}><Input data-autofocus aria-label={$t('302.AI API Key')} type={showKey?'text':'password'} bind:value={apiKey} placeholder="sk-…" autocomplete="off" spellcheck="false" disabled={busy} oninput={()=>error=''}/><Button.Root type="button" class="quick-setup-eye" aria-label={showKey?$t('隐藏密钥'):$t('显示密钥')} aria-pressed={showKey} onclick={()=>showKey=!showKey}>{@html icon(showKey?'eye-off':'eye')}</Button.Root></div>
   <div class="quick-setup-status">{#if error}<p role="alert" class="form-error">{error}</p>{:else if status}<p role="status"><i class="quick-setup-dot" aria-hidden="true"></i>{status}</p>{/if}</div>
  </div>
  <div class="quick-setup-plan">
   <p class="quick-setup-plan-title">{$t('将自动写入')}</p>
   <dl>
    <div><dt>{$t('问答模型')}</dt><dd>{quickSetup.answerModel}</dd></div>
    <div><dt>{$t('决策模型')}</dt><dd class:kept={jevSaved}>{jevSaved?$t('已配置，保持不变'):quickSetup.jevModel}</dd></div>
    <div><dt>{$t('文档解析')}</dt><dd class:kept={mineruSaved}>{mineruSaved?$t('已配置，保持不变'):'MinerU · 302.AI'}</dd></div>
   </dl>
  </div>
 </form>
 <div class="dialog-footer">{#if onManual}<Button.Root type="button" class="settings-text-button" onclick={onManual} disabled={busy}>{$t('手动配置')}</Button.Root>{/if}<Button.Root type="button" class="primary" onclick={run} disabled={busy}>{busy?$t('正在测试…'):$t('测试并保存')}</Button.Root></div>
</Modal>

<style>
 .quick-setup-head{display:flex;align-items:center;justify-content:space-between}
 .quick-setup-body{display:flex;flex-direction:column;gap:16px}
 .quick-setup-intro{margin:0;font-size:var(--text-caption);line-height:1.6;color:var(--color-secondary)}
 .quick-setup-field{display:flex;flex-direction:column;gap:7px}
 .quick-setup-label{display:flex;align-items:center;justify-content:space-between;gap:12px;font-size:var(--text-ui);font-weight:500}
 .quick-setup-label :global(.quick-setup-link){display:inline-flex;align-items:center;gap:4px;margin-right:-6px;padding:3px 6px;border-radius:5px;font-size:12px;font-weight:400;color:var(--color-secondary)}
 .quick-setup-label :global(.quick-setup-link:hover){color:var(--color-ink);background:var(--color-hover)}
 .quick-setup-label :global(.quick-setup-link .icon){width:13px;height:13px}
 .quick-setup-key{display:flex;align-items:center;height:38px;padding:0 6px 0 12px;border:1px solid var(--color-rule);border-radius:8px;background:var(--color-panel);transition:border-color 120ms ease,box-shadow 120ms ease}
 .quick-setup-key:focus-within{border-color:var(--color-secondary);box-shadow:0 0 0 3px var(--color-control-shadow)}
 .quick-setup-key.invalid{border-color:var(--color-danger)}
 .quick-setup-key :global(.ui-input){flex:1;min-width:0;height:100%;margin:0;padding:0;border:0;border-radius:0;background:transparent;box-shadow:none;outline:0;font-size:var(--text-ui)}
 .quick-setup-key :global(.ui-input:focus){border:0;box-shadow:none;outline:0}
 .quick-setup-key :global(.quick-setup-eye){display:grid;place-items:center;width:28px;height:28px;border-radius:6px;color:var(--color-muted)}
 .quick-setup-key :global(.quick-setup-eye:hover){color:var(--color-ink);background:var(--color-hover)}
 .quick-setup-key :global(.quick-setup-eye .icon){width:15px;height:15px}
 .quick-setup-status{min-height:18px;font-size:12px;color:var(--color-secondary);overflow-wrap:anywhere}
 .quick-setup-status p{margin:0;display:flex;align-items:center;gap:6px;font-size:12px;line-height:1.5}
 .quick-setup-dot{width:6px;height:6px;border-radius:50%;background:currentColor;animation:quick-setup-pulse 1.2s ease-in-out infinite}
 @keyframes quick-setup-pulse{50%{opacity:.3}}
 @media(prefers-reduced-motion:reduce){.quick-setup-dot{animation:none}}
 .quick-setup-plan{padding:10px 14px 12px;border:1px solid var(--color-rule);border-radius:10px;background:var(--color-panel)}
 .quick-setup-plan-title{margin:0 0 6px;font-size:11px;letter-spacing:.02em;color:var(--color-muted)}
 .quick-setup-plan dl{margin:0;display:grid;gap:6px}
 .quick-setup-plan dl>div{display:flex;justify-content:space-between;gap:12px;font-size:var(--text-caption)}
 .quick-setup-plan dt{color:var(--color-secondary)}
 .quick-setup-plan dd{margin:0;color:var(--color-ink);font-variant-numeric:tabular-nums}
 .quick-setup-plan dd.kept{color:var(--color-muted)}
</style>
