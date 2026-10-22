<script lang="ts">
 import {Collapsible} from 'bits-ui';
 import {t} from '../lib/i18n';
 import {icon} from '../lib/icons';
 import catalog from '../lib/sag-tools.json';
 import type {ModelProfile} from '../lib/types';
 import Switch from './ui/Switch.svelte';
 export let profile:ModelProfile|null=null;
 export let onChange:(value:ModelProfile)=>void=()=>{};
 const id='builtin:sag';
 const tools=catalog.map(tool=>({...tool,required:new Set<string>(tool.inputSchema.required)}));
 $:selected=profile?.mcp_tools?.[id]??tools.map(tool=>tool.name);
 $:enabled=profile?.sag2_enabled??true;
 function select(name:string,checked:boolean){
  if(!profile)return;
  onChange({...profile,mcp_tools:{...profile.mcp_tools,[id]:checked?[...selected,name]:selected.filter(tool=>tool!==name)}});
 }
</script>
<div class="model-mcp-server sag-tools">
 <div class="setting-row"><div><strong>{$t("SAG（内置 MCP）")}</strong><p>{$t("本地文件检索、当前时间与计算，无需配置连接。")}</p></div>{#if profile}<Switch label={$t("启用 SAG 内置工具")} checked={enabled} onCheckedChange={checked=>onChange({...profile!,sag2_enabled:checked})}/>{/if}</div>
 {#if enabled||!profile}<Collapsible.Root class="model-tool-disclosure">
  <div class="model-tool-toolbar"><Collapsible.Trigger class="model-tool-toggle" aria-label={$t("SAG 的工具列表")}><span class="model-tool-chevron" aria-hidden="true">{@html icon('chevron')}</span><span>{$t("工具列表")}</span><small>{#if profile}{$t("共 {value0} 个工具 · 已启用 {value1} 个",{value0:tools.length,value1:selected.length})}{:else}{$t("共 {value0} 个工具",{value0:tools.length})}{/if}</small></Collapsible.Trigger></div>
  <Collapsible.Content class="model-tool-content"><div class="model-mcp-tools">
   {#each tools as tool(tool.name)}<div class="setting-row"><div><strong>{tool.name}</strong><p>{$t(tool.description)}</p><p class="settings-help">{$t("参数")}: {Object.entries(tool.inputSchema.properties).map(([name,definition])=>`${name}${tool.required.has(name)?'*':''}${'default' in definition?`=${definition.default}`:''}`).join(' · ')}</p></div>{#if profile}<Switch label={$t("启用 SAG / {value0}",{value0:tool.name})} checked={selected.includes(tool.name)} onCheckedChange={checked=>select(tool.name,checked)}/>{/if}</div>{/each}
  </div></Collapsible.Content>
 </Collapsible.Root>{/if}
</div>
