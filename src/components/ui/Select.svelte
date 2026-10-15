<script lang="ts">
 import {Select,IsUsingKeyboard} from 'bits-ui';
 import {icon} from '../../lib/icons';
 import {surface} from '../../lib/motion';
 let {value=$bindable(''),items,label,class:className='',placeholder='',disabled=false,onValueChange=()=>{}}:{value?:string;items:{value:string;label:string}[];label:string;class?:string;placeholder?:string;disabled?:boolean;onValueChange?:(value:string)=>void}=$props();
 const keyboard=new IsUsingKeyboard();
</script>
<Select.Root type="single" bind:value {items} {onValueChange} {disabled}>
 <Select.Trigger class={`ui-select ${className}`} aria-label={label}><span>{items.find(item=>item.value===value)?.label ?? (placeholder||value)}</span>{@html icon('down')}</Select.Trigger>
 <Select.Portal>
  <Select.Content align="center" sideOffset={5} collisionPadding={12} forceMount>
   {#snippet child({props,wrapperProps,open})}
    {#if open}
     <div {...wrapperProps}>
      <div {...props} class="ui-select-menu" transition:surface|global={{keyboard:keyboard.current}}>
       <Select.Viewport>
        {#each items as item}
         <Select.Item value={item.value} label={item.label} class="ui-select-option">
          {#snippet children({selected})}<span>{item.label}</span><span class="ui-select-check">{#if selected}{@html icon('check')}{/if}</span>{/snippet}
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
