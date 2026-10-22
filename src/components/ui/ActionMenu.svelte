<script lang="ts">
 import {t} from '../../lib/i18n';
 import {DropdownMenu,IsUsingKeyboard} from 'bits-ui';
 import {surface} from '../../lib/motion';
 import {tick} from 'svelte';
 import {icon} from '../../lib/icons';
 export type MenuState={x:number;y:number;trigger:HTMLElement;items:{label:string;glyph:string;run:()=>void}[]};
 let {menu,onClose}:{menu:MenuState;onClose:()=>void}=$props();
 const keyboard=new IsUsingKeyboard();
</script>
<DropdownMenu.Root open={true} onOpenChange={open=>{if(!open)onClose()}}>
 <DropdownMenu.Portal>
  <DropdownMenu.ContentStatic forceMount preventScroll={false} id="context-menu" aria-label={$t("操作菜单")} style={`left:${menu.x}px;top:${menu.y}px`}
   onCloseAutoFocus={event=>{event.preventDefault();if(menu.trigger.isConnected)menu.trigger.focus({preventScroll:true})}}>
   {#snippet child({props})}
   <div {...props} oncontextmenu={(event:MouseEvent)=>event.preventDefault()} transition:surface|global={{keyboard:keyboard.current}}>
   {#each menu.items as item}
    <DropdownMenu.Item class="menu-item" textValue={item.label} onSelect={()=>{onClose();void tick().then(()=>item.run())}}>
     {@html icon(item.glyph)}<span>{item.label}</span>
    </DropdownMenu.Item>
   {/each}
   </div>
   {/snippet}
  </DropdownMenu.ContentStatic>
 </DropdownMenu.Portal>
</DropdownMenu.Root>
