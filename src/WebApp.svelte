<script lang="ts">
 import {onMount} from 'svelte';
 import {connectWeb,disconnectWeb,onWebAccessCodeRequired,onWebSignOut,savedWebCode} from './lib/web-connection';
 import {logo} from './lib/icons';
 import {t,setLanguage,resolveBrowserLanguage} from './lib/i18n';
 import App from './App.svelte';
 let connected=false,checking=true,busy=false,requiresCode=true,code='',error='';
 async function signIn(accessCode:string){
  busy=true;error='';
  try{const session=await connectWeb(accessCode.trim());if(session){requiresCode=session.accessCodeRequired;connected=true;code='';}}
  catch(e){error=e instanceof Error?e.message:String(e);}
  finally{busy=false;}
 }
 function signOut(){disconnectWeb();connected=false;}
 onMount(()=>{
  setLanguage(resolveBrowserLanguage(navigator.language));
  void signIn(savedWebCode()).finally(()=>{checking=false;});
  const stopSignOut=onWebSignOut(()=>{connected=false;error='请重新输入网页访问码。';});
  const stopAccessCode=onWebAccessCodeRequired(required=>requiresCode=required);
  return ()=>{stopSignOut();stopAccessCode();};
 });
</script>

{#if connected}
 <App onWebSignOut={requiresCode?signOut:undefined}/>
{:else if checking}
 <main class="web-login" role="status">{$t('正在连接…')}</main>
{:else}
 <main class="web-login">
  <form on:submit|preventDefault={()=>signIn(code)}>
   <div class="web-brand">{@html logo}<h1>SAG Wiki</h1></div>
   <label for="web-code">{$t('网页访问码')}</label>
   <input id="web-code" type="password" bind:value={code} autocomplete="current-password" spellcheck="false" required/>
   {#if error}<p class="form-error" role="alert">{$t(error)}</p>{/if}
   <button class="primary" disabled={busy||!code.trim()}>{$t(busy?'正在连接…':'进入知识库')}</button>
  </form>
 </main>
{/if}

<style>
 .web-login{display:grid;place-items:center;height:100dvh;overflow:auto;padding:16px;background:var(--color-panel)}
 form{box-sizing:border-box;width:min(100%,380px);display:grid;gap:12px;padding:clamp(16px,6vw,32px);border:1px solid var(--color-rule);border-radius:16px;background:var(--color-paper)}
 .web-brand{display:flex;align-items:center;gap:12px;margin-bottom:14px}.web-brand :global(img){width:32px;height:32px}.web-brand h1{margin:0;font-size:23px}
 label{font-size:13px}input{min-width:0;width:100%;padding:9px 12px}.primary{margin-top:4px;min-height:36px}
</style>
