<script lang="ts">
 import {Switch,IsUsingKeyboard} from 'bits-ui';
 let {checked=$bindable(false),label,disabled=false,onCheckedChange}:{checked?:boolean;label:string;disabled?:boolean;onCheckedChange?:(checked:boolean)=>void}=$props();
 const keyboard=new IsUsingKeyboard();
</script>
<Switch.Root bind:checked {onCheckedChange} {disabled} aria-label={label} class="ui-switch" data-keyboard={keyboard.current}>
 <Switch.Thumb class="ui-switch-thumb"/>
</Switch.Root>
