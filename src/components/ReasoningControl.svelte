<script lang="ts">
 import {t} from '../lib/i18n';
 import {Popover,Slider,Command,Button,IsUsingKeyboard} from 'bits-ui';
 import {tick} from 'svelte';
 import {icon} from '../lib/icons';
 import {surface} from '../lib/motion';
 import {reasoningLevels,type ReasoningEffort} from '../lib/reasoning';
 let {model,models=[],endpoint,value=null,disabled=false,onChange,onModelChange=()=>{},onCommit=()=>{}}:{model:string;models?:string[];endpoint:string;value?:ReasoningEffort|null;disabled?:boolean;onChange:(value:ReasoningEffort|null)=>void;onModelChange?:(model:string)=>void;onCommit?:()=>void}=$props();
 let open=$state(false);
 let panel=$state<'reasoning'|'models'>('reasoning');
 let modelList=$state<HTMLDivElement|null>(null);
 const keyboard=new IsUsingKeyboard();
 const levels=$derived(reasoningLevels(model,endpoint));
 const index=$derived(Math.max(0,levels.findIndex(level=>level.value===value)));
 const unsupported=$derived(value!==null&&!levels.some(level=>level.value===value));
 const label=$derived(unsupported?$t("未适配"):levels[index].value==='none'?$t('reasoning.off'):$t(levels[index].label));
 const bubbles=Array.from({length:14},()=>({
  y:12+Math.random()*72,size:1.3+Math.random()*1.8,
  duration:3.5+Math.random()*5,delay:-Math.random()*9,
  opacity:.25+Math.random()*.4,rest:Math.random()*96,
 }));
 async function showModels(){panel='models';await tick();modelList?.focus()}
 function chooseModel(name:string){onModelChange(name);open=false}
</script>
<Popover.Root bind:open onOpenChange={isOpen=>{if(isOpen)panel='reasoning'}}>
 <Popover.Trigger type="button" class={`reasoning-button ${models.length?'reasoning-combined':''}`} aria-label={models.length?$t("模型与思考强度"):$t("思考强度")} {disabled}>
  {#if models.length}<span class="reasoning-trigger-model">{model}</span>{/if}<span class="reasoning-trigger-effort">{label}</span>{@html icon('down')}
 </Popover.Trigger>
 <Popover.Portal>
  <Popover.Content side="top" align="center" sideOffset={8} collisionPadding={12} forceMount>
   {#snippet child({props,wrapperProps,open})}
    {#if open}<div {...wrapperProps}>
     {#if panel==='models'}
      <div {...props} class="reasoning-model-picker" in:surface|global={{keyboard:keyboard.current}}>
       <p class="reasoning-model-heading">{$t("选择模型")}</p>
       <Command.Root bind:ref={modelList} label={$t("选择模型")} value={model} shouldFilter={false} tabindex={0} class="reasoning-model-command">
        <Command.List aria-label={$t("模型列表")} class="reasoning-model-list">
         {#each models as name}
          <Command.Item value={name} class="reasoning-model-option" onSelect={()=>chooseModel(name)}><span>{name}</span>{#if name===model}{@html icon('check')}{/if}</Command.Item>
         {/each}
        </Command.List>
       </Command.Root>
      </div>
     {:else}<div {...props} class="reasoning-popover" in:surface|global={{keyboard:keyboard.current}}>
     <button class="reasoning-reset" type="button" aria-label={$t("恢复默认思考强度")} title={$t("恢复默认")} onclick={()=>{onChange(null);onCommit()}}>{@html icon('refresh')}</button>
     <p class="reasoning-current">{label}</p>
     {#if models.length}<div class="reasoning-model-row"><Button.Root class="reasoning-model-button" type="button" aria-label={$t("选择问答模型")} onclick={showModels} onkeydown={(event:KeyboardEvent)=>{if(event.key==='ArrowDown'){event.preventDefault();showModels()}}}>{model}{@html icon('chevron')}</Button.Root></div>{:else}<p class="reasoning-model">{model}</p>{/if}
     {#if levels.length>1}
      <Slider.Root type="single" class="reasoning-slider" min={0} max={levels.length-1} step={1} value={index} onValueChange={i=>onChange(levels[i].value)} onValueCommit={()=>onCommit()}>
       <Slider.Range class="reasoning-slider-range">
        <span class="reasoning-bubbles" aria-hidden="true">
         {#each bubbles as bubble}<span class="reasoning-bubble" style={`--bubble-y:${bubble.y}%;--bubble-size:${bubble.size}px;--bubble-duration:${bubble.duration}s;--bubble-delay:${bubble.delay}s;--bubble-opacity:${bubble.opacity};--bubble-rest:${bubble.rest}%`}></span>{/each}
        </span>
       </Slider.Range>
       {#each levels as level,i}<Slider.Tick index={i} class="reasoning-slider-tick" aria-hidden="true"/>{/each}
       <Slider.Thumb index={0} class="reasoning-slider-thumb" aria-label={$t("模型思考强度")} aria-valuetext={label}/>
      </Slider.Root>
     {:else if !unsupported}<p class="reasoning-help">{$t("此服务与模型暂未适配强度设置，沿用默认值。")}</p>{/if}
     {#if unsupported}<p class="reasoning-help">{$t("当前服务与模型不支持已保存的档位，请重新选择或恢复默认。")}</p>{/if}
    </div>{/if}</div>{/if}
   {/snippet}
  </Popover.Content>
 </Popover.Portal>
</Popover.Root>
