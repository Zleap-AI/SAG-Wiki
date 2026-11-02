export type QuickSearchPreferences={enabled:boolean;shortcut:string};
export type QuickSearchConfiguration={preferences:QuickSearchPreferences;error:string|null};
export function shortcutFromKey(event:KeyboardEvent):string|null{
 if(event.isComposing||event.keyCode===229||event.repeat||!(event.ctrlKey||event.altKey||event.metaKey))return null;
 const key=/^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-9]|2[0-4])|Space|Arrow(Up|Down|Left|Right)|Home|End|PageUp|PageDown|Enter|Backspace|Delete|Tab)$/.test(event.code)?event.code:null;
 if(!key)return null;
 return [...(event.metaKey?['Super']:[]),...(event.ctrlKey?['Control']:[]),...(event.altKey?['Alt']:[]),...(event.shiftKey?['Shift']:[]),key].join('+');
}
export function shortcutLabel(shortcut:string):string{
 const mac=/Mac|iPhone|iPad/.test(navigator.platform);
 return shortcut.split('+').map(part=>({CommandOrControl:mac?'⌘':'Ctrl',Super:mac?'⌘':'Win',Control:'Ctrl',Alt:mac?'⌥':'Alt',Shift:mac?'⇧':'Shift',Space:'Space',ArrowUp:'↑',ArrowDown:'↓',ArrowLeft:'←',ArrowRight:'→'}[part]??part.replace(/^(Key|Digit)/,''))).join(' + ');
}
