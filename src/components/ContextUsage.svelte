<script lang="ts">
 import {t} from '../lib/i18n';
 import {Popover,IsUsingKeyboard} from 'bits-ui';
 import type {AnswerContext} from '../lib/types';
 import {compactNumber} from '../lib/domain';
 import {surface} from '../lib/motion';
 let {context,limit,loading=false}:{context?:AnswerContext|null;limit:number;loading?:boolean}=$props();
 const keyboard=new IsUsingKeyboard();
 const used=$derived(context?.estimated_input??0);
 const capacity=$derived(context?.limit??limit);
 const percent=$derived(Math.min(100,Math.max(0,Math.round(used/capacity*100))));
</script>
<Popover.Root>
 <Popover.Trigger type="button" class="context-usage" aria-label={loading?$t("上下文占用：正在计算"):$t("上下文占用约 {value0}%",{value0:percent})} title={loading?$t("正在计算上下文占用"):$t("上下文占用约 {value0}%",{value0:percent})}>
  <span class="context-ring" class:loading style={`--context-fill:${percent}%`} aria-hidden="true"></span>
 </Popover.Trigger>
 <Popover.Portal>
  <Popover.Content side="top" align="center" sideOffset={8} collisionPadding={12} forceMount>
   {#snippet child({props,wrapperProps,open})}
    {#if open}<div {...wrapperProps}><div {...props} class="context-popover" transition:surface|global={{keyboard:keyboard.current}}>
     <strong>{$t("上下文窗口")}</strong>
     {#if loading}<p>{$t("正在计算本轮占用…")}</p>
     {:else if context}<p>{$t("{value0} / {value1} tokens · 约 {value2}%",{value0:compactNumber(used),value1:compactNumber(capacity),value2:percent})}</p><small>{$t("最近一轮输入的分词估算，包含提示词、对话和工具证据。")}</small>
     {:else}<p>{$t("上限 {value0} tokens",{value0:compactNumber(capacity)})}</p><small>{$t("发送消息后显示输入占用。")}</small>{/if}
    </div></div>{/if}
   {/snippet}
  </Popover.Content>
 </Popover.Portal>
</Popover.Root>
