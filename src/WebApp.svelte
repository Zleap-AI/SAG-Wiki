<script lang="ts">
 import {onMount} from 'svelte';
 import {connectWeb,disconnectWeb,onWebSignOut,savedWebCode} from './lib/web-connection';
 import {logo} from './lib/icons';
 import App from './App.svelte';
 let connected=false,busy=false,code='',error='';
 let english=!navigator.language.startsWith('zh');
 const label=(zh:string,en:string)=>english?en:zh;
 async function signIn(accessCode:string){
  busy=true;error='';
  try{await connectWeb(accessCode.trim());connected=true;code='';}
  catch(e){error=e instanceof Error?e.message:String(e);}
  finally{busy=false;}
 }
 function signOut(){disconnectWeb();connected=false;}
 onMount(()=>{
  const saved=savedWebCode();if(saved)void signIn(saved);
  return onWebSignOut(()=>{connected=false;error=label('请重新输入网页访问码。','Enter the web access code again.');});
 });
</script>

{#if connected}
 <App onWebSignOut={signOut}/>
{:else}
 <main class="web-login">
  <form on:submit|preventDefault={()=>signIn(code)}>
   <div class="web-brand">{@html logo}<h1>SAG Wiki</h1></div>
   <label for="web-code">{label('网页访问码','Web access code')}</label>
   <input id="web-code" type="password" bind:value={code} autocomplete="current-password" spellcheck="false" required/>
   {#if error}<p class="form-error" role="alert">{error}</p>{/if}
   <button class="primary" disabled={busy||!code.trim()}>{busy?label('正在连接…','Connecting…'):label('进入知识库','Open knowledge base')}</button>
  </form>
 </main>
{/if}

<style>
 .web-login{display:grid;place-items:center;height:100dvh;overflow:auto;padding:16px;background:var(--color-panel)}
 form{box-sizing:border-box;width:min(100%,380px);display:grid;gap:12px;padding:clamp(16px,6vw,32px);border:1px solid var(--color-rule);border-radius:16px;background:var(--color-paper)}
 .web-brand{display:flex;align-items:center;gap:12px;margin-bottom:14px}.web-brand :global(img){width:32px;height:32px}.web-brand h1{margin:0;font-size:23px}
 label{font-size:13px}input{min-width:0;width:100%;padding:9px 12px}.primary{margin-top:4px;min-height:36px}
</style>
