// Throwaway UI prototype. No backend, network probes, real keys, or persistence.
const paths={
 settings:'<path d="M4 6h16M4 12h16M4 18h16"/><path d="M8 3v6m8 0v6M10 15v6"/>',
 sun:'<circle cx="12" cy="12" r="4"/><path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.4 1.4m11.2 11.2L19 19M5 19l1.4-1.4M17.6 6.4 19 5"/>',
 folder:'<path d="M3 7V5a2 2 0 0 1 2-2h4l3 3h7a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/>',
 globe:'<circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18"/>',
 panel:'<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18"/>',
 plug:'<path d="M8 2v5m8-5v5M6 7h12v4a6 6 0 0 1-12 0Zm6 10v5"/>',
 chat:'<path d="M21 15a3 3 0 0 1-3 3H8l-5 4V5a3 3 0 0 1 3-3h12a3 3 0 0 1 3 3Z"/><path d="M7 7h10M7 11h7"/>',
 search:'<circle cx="10.5" cy="10.5" r="7"/><path d="m16 16 5 5"/>',
 code:'<path d="m7 7-5 5 5 5m10-10 5 5-5 5m-7 3 4-16"/>',
 network:'<rect x="8" y="2" width="8" height="6" rx="1"/><rect x="2" y="16" width="7" height="6" rx="1"/><rect x="15" y="16" width="7" height="6" rx="1"/><path d="M12 8v4M5.5 16v-4h13v4"/>',
 info:'<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7h.01"/>',
 close:'<path d="m6 6 12 12M6 18 18 6"/>',
 copy:'<rect x="8" y="8" width="12" height="13" rx="2"/><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h3"/>',
 plus:'<path d="M12 5v14M5 12h14"/>',
 chevron:'<path d="m9 5 7 7-7 7"/>',
 check:'<path d="m5 12 4 4L19 6"/>',
 key:'<circle cx="8" cy="8" r="5"/><path d="m11.5 11.5 9 9M16 16l3-3m-7 7 3-3"/>',
 alert:'<path d="m10.3 3.9-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.7-3.1l-8-14a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4m0 4h.01"/>',
 external:'<path d="M15 3h6v6m0-6L11 13M10 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5"/>'
};
const icon=name=>`<svg viewBox="0 0 24 24" aria-hidden="true">${paths[name]}</svg>`;
const escape=value=>String(value).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const labels={A:'分组',B:'分页',C:'双栏'};
const initialVariant=new URL(location.href).searchParams.get('variant');
let variant=Object.hasOwn(labels,initialVariant)?initialVariant:'A',tab='connection';
let enabled=true,autostart=false,host='0.0.0.0',port=24681;
let keys=[{id:'demo-a',name:'内部检索',suffix:'4d2a',date:'2026/11/12'},{id:'demo-b',name:'项目助手',suffix:'8e71',date:'2026/11/12'}];
let secret='',revoking='',toastTimer;
const content=document.querySelector('#content'),keyDialog=document.querySelector('#key-dialog'),revokeDialog=document.querySelector('#revoke-dialog');
const nav=[['通用','settings'],['外观','sun'],['文件设置','folder'],['网页剪藏','globe'],['快捷键','panel'],['模型设置','plug'],['问答设置','chat'],['搜索设置','search'],['CLI 设置','code'],['API 服务','network'],['数据与隐私','globe'],['关于','info']];
document.querySelector('#settings-nav').innerHTML=nav.map(([name,glyph])=>`<button type="button" class="${name==='API 服务'?'active':''}" ${name==='API 服务'?'aria-current="page"':''} data-action="nav" data-name="${name}">${icon(glyph)}<span>${name}</span></button>`).join('');
document.querySelectorAll('[data-action="close-settings"].icon-button,[data-action="close-key"],[data-action="close-revoke"].icon-button').forEach(button=>button.innerHTML=icon('close'));

function toggle(label,action,checked){return `<button type="button" class="switch" role="switch" aria-label="${label}" aria-checked="${checked}" data-action="${action}"></button>`;}
function service(){return `<div class="row service-row"><div class="row-title"><strong>启用 API</strong><span class="status ${enabled?'running':''}">${enabled?'已运行':'已关闭'}</span></div>${toggle('启用 API','toggle-api',enabled)}</div>`;}
function addresses(){return [['本机地址',`http://127.0.0.1:${port}`],['局域网地址',`http://192.168.1.10:${port}`]].map(([label,value])=>`<div class="row address-row ${enabled?'':'is-off'}"><span>${label}</span><div class="address-value"><code title="${value}">${value}</code><button class="icon-button" data-action="copy" data-value="${value}" aria-label="复制${label}" title="复制${label}">${icon('copy')}</button></div></div>`).join('');}
function configuration(alwaysOpen=false){const fields=`<div class="fields"><label class="field">监听地址<input id="host" value="${escape(host)}" required ${enabled?'disabled':''} spellcheck="false"></label><label class="field">端口<input id="port" type="number" value="${port}" min="1" max="65535" required ${enabled?'disabled':''}></label>${enabled?'<p class="fields-note">关闭 API 后可修改监听地址和端口。</p>':''}</div>`;return alwaysOpen?fields:`<details class="connection-config"><summary>${icon('chevron')}连接配置</summary>${fields}</details>`;}
function startup(){return `<div class="row"><div><strong>开机自启</strong><p>登录后按当前开关状态恢复。</p></div>${toggle('开机自启','toggle-autostart',autostart)}</div>`;}
function missingKey(){return keys.length?'':`<p class="no-key">${icon('alert')}<span>尚无 API Key，请求将返回未授权（401）。</span></p>`;}
function keyTable(){if(!keys.length)return `<div class="empty-keys">${icon('key')}<h3>暂无 API Key</h3><p>创建 Key 后，其他程序即可调用。</p><button class="outline" data-action="create-key">创建 Key</button></div>`;return `<table class="key-table"><thead><tr><th>名称</th><th>Key</th><th><span class="visually-hidden">操作</span></th></tr></thead><tbody>${keys.map(key=>`<tr><td><span class="key-name" title="${escape(key.name)}">${escape(key.name)}</span><div class="key-date">${variant==='C'?`<code>•••• ${key.suffix}</code><span>·</span>`:''}<span>${key.date}</span></div></td><td><code>sag_••••••••${key.suffix}</code></td><td><button class="revoke" data-action="revoke-key" data-id="${key.id}" aria-label="撤销 ${escape(key.name)}">撤销</button></td></tr>`).join('')}</tbody></table>`;}
function keySection(){return `<section class="key-section"><div class="section-heading"><h3>API Key</h3><button class="outline" data-action="create-key">${icon('plus')}创建 Key</button></div><div class="group">${keyTable()}</div></section>`;}
function exampleText(){return `curl http://192.168.1.10:${port}/v1/search \\\n  -H 'Authorization: Bearer <API_KEY>' \\\n  -H 'Content-Type: application/json' \\\n  -d '{"query":"项目如何部署","mode":"deep"}'`;}
function example(){return `<details class="example"><summary>${icon('chevron')}调用示例</summary><div class="code-box"><button class="icon-button" data-action="copy-example" aria-label="复制调用示例" title="复制调用示例">${icon('copy')}</button><pre>${escape(exampleText())}</pre></div></details>`;}
function heading(){return `<header class="page-heading"><div class="heading-row"><h1>API 服务</h1><a class="docs-link" href="http://127.0.0.1:3236/zh/docs/api" target="_blank" rel="noopener noreferrer">API 文档${icon('external')}</a></div><p>让本机和局域网程序搜索知识库。</p></header>`;}
function VariantA(){return `${heading()}<div class="group">${service()}${addresses()}${configuration()}${startup()}</div>${missingKey()}${keySection()}${example()}`;}
function VariantB(){return `${heading()}<div class="tabbar" role="tablist" aria-label="API 设置"><button role="tab" aria-selected="${tab==='connection'}" data-action="tab" data-tab="connection">服务连接</button><button role="tab" aria-selected="${tab==='keys'}" data-action="tab" data-tab="keys">API Key<span class="tab-count">${keys.length}</span></button></div>${tab==='connection'?`<div class="group">${service()}${addresses()}</div><div class="group">${configuration(true)}${startup()}</div>${missingKey()}${example()}`:`<p class="b-key-description">所有 Key 均可搜索全部知识库，完整 Key 仅在创建时显示。</p>${missingKey()}${keySection()}`}`;}
function VariantC(){return `${heading()}<div class="two-column"><section class="left-rail"><div class="group">${service()}${addresses()}${configuration()}${startup()}</div>${missingKey()}</section><section>${keySection()}${example()}</section></div>`;}
function render(){
 content.className=`settings-content variant-${variant.toLowerCase()}`;
 content.innerHTML=({A:VariantA,B:VariantB,C:VariantC}[variant])();
 document.querySelectorAll('[data-action="variant"]').forEach(button=>{button.classList.toggle('active',button.dataset.variant===variant);button.setAttribute('aria-pressed',String(button.dataset.variant===variant));});
 const state={variant,layout:labels[variant],enabled,running:enabled,autostart,host,port,keys:keys.map(({id,name,suffix})=>({id,name,suffix})),decision_configured:true,unauthorized:enabled&&!keys.length};
 document.querySelector('#prototype-state').textContent=JSON.stringify(state,null,2);
 console.info('API settings prototype',state);
}
function changeVariant(value){variant=value;const url=new URL(location.href);url.searchParams.set('variant',variant);history.replaceState(null,'',url);content.scrollTop=0;render();}
function toast(message){clearTimeout(toastTimer);const node=document.querySelector('#toast');node.textContent=message;node.hidden=false;toastTimer=setTimeout(()=>node.hidden=true,2200);}
async function copy(value){try{await navigator.clipboard.writeText(value);toast('已复制');}catch(error){toast(`复制失败：${error.message}`);}}
function openKey(){
 secret='';document.querySelector('#key-dialog-title').textContent='创建 API Key';
 document.querySelector('#key-dialog-body').innerHTML='<div class="dialog-body"><label class="field" for="key-name">名称<input id="key-name" name="key-name" required maxlength="60" placeholder="例如：内部检索" autocomplete="off"></label><p class="muted">完整 Key 仅在创建时显示一次。</p></div>';
 document.querySelector('#key-dialog-footer').innerHTML='<button type="button" class="outline" data-action="close-key">取消</button><button type="submit" class="primary">创建 Key</button>';
 keyDialog.showModal();document.querySelector('#key-name').focus();
}
document.querySelector('#key-form').addEventListener('submit',event=>{
 event.preventDefault();const input=document.querySelector('#key-name');if(!input)return;
 const name=input.value.trim();if(!name){input.setCustomValidity('请输入名称');input.reportValidity();return;}
 const id=crypto.randomUUID();secret=`sag_demo_${id.replaceAll('-','')}${crypto.randomUUID().replaceAll('-','')}`;const suffix=secret.slice(-4);
 keys.push({id,name,suffix,date:'2026/11/12'});render();
 document.querySelector('#key-dialog-title').textContent='Key 已创建';
 document.querySelector('#key-dialog-body').innerHTML=`<div class="dialog-body"><span class="created-mark">${icon('check')}</span><p>请保存 <strong>${escape(name)}</strong> 的 API Key。</p><div class="secret-box"><code>${secret}</code><button type="button" class="icon-button" data-action="copy-secret" aria-label="复制完整 Key" title="复制 Key">${icon('copy')}</button></div><p class="muted">关闭后将无法再次查看完整 Key。</p></div>`;
 document.querySelector('#key-dialog-footer').innerHTML='<button type="button" class="primary" data-action="close-key">我已保存</button>';
 document.querySelector('#key-dialog-footer button').focus();
});
keyDialog.addEventListener('close',()=>{secret='';document.querySelector('#key-dialog-body').innerHTML='';});
document.addEventListener('input',event=>{if(event.target.id==='key-name')event.target.setCustomValidity('');});
document.addEventListener('change',event=>{if(event.target.id==='host'){if(!event.target.reportValidity())return;host=event.target.value.trim();render();}if(event.target.id==='port'){if(!event.target.reportValidity())return;port=Number(event.target.value);render();}});
document.addEventListener('click',event=>{
 const button=event.target.closest('[data-action]');if(!button)return;
 switch(button.dataset.action){
 case 'toggle-api':enabled=!enabled;render();toast(enabled?'API 已开启':'API 已关闭');break;
 case 'toggle-autostart':autostart=!autostart;render();toast(autostart?'已开启登录自启':'已关闭登录自启');break;
 case 'copy':void copy(button.dataset.value);break;
 case 'copy-example':void copy(exampleText());break;
 case 'copy-secret':void copy(secret);break;
 case 'create-key':openKey();break;
 case 'close-key':keyDialog.close();break;
 case 'revoke-key':{
  revoking=button.dataset.id;const key=keys.find(item=>item.id===revoking);
  document.querySelector('#revoke-title').textContent=`撤销“${key.name}”？`;
  document.querySelector('#revoke-description').textContent='使用此 Key 的搜索将被取消，后续请求将无法通过认证。';
  document.querySelector('#revoke-note').textContent=keys.length===1?'API 仍保持当前开关状态。没有有效 Key 时，请求返回未授权（401）。':'其他 Key 和桌面端、CLI 的任务不受影响。';
  revokeDialog.showModal();break;
 }
 case 'close-revoke':revokeDialog.close();break;
 case 'confirm-revoke':keys=keys.filter(item=>item.id!==revoking);revokeDialog.close();render();toast('Key 已撤销');break;
 case 'tab':tab=button.dataset.tab;render();break;
 case 'variant':changeVariant(button.dataset.variant);break;
 case 'prev-variant':changeVariant(['A','B','C'][(Object.keys(labels).indexOf(variant)+2)%3]);break;
 case 'next-variant':changeVariant(['A','B','C'][(Object.keys(labels).indexOf(variant)+1)%3]);break;
 case 'close-settings':document.querySelector('#backdrop').hidden=true;document.querySelector('#reopen').hidden=false;break;
 case 'open-settings':document.querySelector('#backdrop').hidden=false;document.querySelector('#reopen').hidden=true;break;
 case 'nav':if(button.dataset.name!=='API 服务')toast('此原型仅演示 API 设置');break;
 }
});
document.addEventListener('keydown',event=>{
 if(keyDialog.open||revokeDialog.open||event.target.closest('input,textarea,select,[contenteditable=true]'))return;
 if(event.key==='ArrowRight'||event.key==='ArrowLeft'){event.preventDefault();const index=Object.keys(labels).indexOf(variant);changeVariant(Object.keys(labels)[(index+(event.key==='ArrowRight'?1:2))%3]);}
});
render();
