<script lang="ts">
 import {Select,IsUsingKeyboard} from 'bits-ui';
 import {icon} from '../../lib/icons';
 import {surface} from '../../lib/motion';
 let {value=$bindable(''),items,label,class:className='',placeholder='',disabled=false,onValueChange=()=>{}}:{value?:string;items:{value:string;label:string;description?:string}[];label:string;class?:string;placeholder?:string;disabled?:boolean;onValueChange?:(value:string)=>void}=$props();
 const keyboard=new IsUsingKeyboard();
 const selectedItem=$derived(items.find(item=>item.value===value));
</script>
<Select.Root type="single" bind:value {items} {onValueChange} {disabled}>
 <Select.Trigger class={`ui-select ${className}`} aria-label={label}>{#if selectedItem?.description}<span class="ui-select-text"><span>{selectedItem.label}</span><small>{selectedItem.description}</small></span>{:else}<span>{selectedItem?.label ?? (placeholder||value)}</span>{/if}{@html icon('down')}</Select.Trigger>
 <Select.Portal>
  <Select.Content align="center" sideOffset={5} collisionPadding={12} forceMount>
   {#snippet child({props,wrapperProps,open})}
    {#if open}
     <div {...wrapperProps}>
      <div {...props} class="ui-select-menu" transition:surface|global={{keyboard:keyboard.current}}>
       <Select.Viewport>
        {#each items as item}
         <Select.Item value={item.value} label={item.label} class="ui-select-option">
          {#snippet children({selected})}{#if item.description}<span class="ui-select-text"><span>{item.label}</span><small>{item.description}</small></span>{:else}<span>{item.label}</span>{/if}<span class="ui-select-check">{#if selected}{@html icon('check')}{/if}</span>{/snippet}
         </Select.Item>
        {/each}
       </Select.Viewport>
      </div>
     </div>
    {/if}
   {/snippet}
  </Select.Content>
 </Select.Portal>
</Select.Root>
