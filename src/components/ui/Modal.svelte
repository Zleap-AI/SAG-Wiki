<script lang="ts">
 import {Dialog,IsUsingKeyboard} from 'bits-ui';
 import {surface} from '../../lib/motion';
 import type {Snippet} from 'svelte';
 let {label,onClose,children,class:className='',overlayClass='',drawer=false}:{label:string;onClose:()=>void;children:Snippet;class?:string;overlayClass?:string;drawer?:boolean}=$props();
 let content:HTMLDivElement|null=$state(null);
 const previous=document.activeElement;
 const keyboard=new IsUsingKeyboard();
</script>
<Dialog.Root open={true} onOpenChange={open=>{if(!open)onClose()}}>
 <Dialog.Portal>
  <Dialog.Overlay forceMount>
 {#snippet child({props})}
 <div {...props} transition:surface|global={{kind:"overlay",keyboard:keyboard.current}} class={drawer?'source-dialog':`modal-backdrop ${overlayClass}`}>
   <Dialog.Content bind:ref={content} forceMount aria-label={label}
    onOpenAutoFocus={event=>{const input=content?.querySelector<HTMLElement>('[data-autofocus]');if(input){event.preventDefault();input.focus()}}}
    onCloseAutoFocus={event=>{event.preventDefault();if(previous instanceof HTMLElement&&previous.isConnected)previous.focus({preventScroll:true})}}>
    {#snippet child({props})}
    <div {...props} class={drawer?`source-panel ${className}`:`dialog-panel ${className}`} transition:surface|global={{kind:drawer?"drawer":"dialog",keyboard:keyboard.current}}>
    <Dialog.Title class="sr-only">{label}</Dialog.Title>
    {@render children()}
    </div>
    {/snippet}
   </Dialog.Content>
 </div>
 {/snippet}
  </Dialog.Overlay>
 </Dialog.Portal>
</Dialog.Root>
