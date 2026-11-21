export function rememberScroll(node:HTMLElement,options:{key:string;enabled:boolean}){
 let position=Number(localStorage.getItem(options.key)??0);
 const restore=()=>{if(options.enabled)node.scrollTop=position};
 const save=()=>{if(options.enabled){position=node.scrollTop;localStorage.setItem(options.key,String(position))}};
 node.addEventListener('scroll',save);
 restore();
 return{
  update(next:typeof options){
   const wasEnabled=options.enabled,keyChanged=next.key!==options.key;
   options=next;
   if(keyChanged)position=Number(localStorage.getItem(options.key)??0);
   if(keyChanged||!wasEnabled)restore();
  },
  destroy(){node.removeEventListener('scroll',save)},
 };
}
