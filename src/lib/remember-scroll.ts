import {userStorage} from './identity';
export function rememberScroll(node:HTMLElement,options:{key:string;enabled:boolean}){
 let position=Number(userStorage.getItem(options.key)??0);
 const restore=()=>{if(options.enabled)node.scrollTop=position};
 const save=()=>{if(options.enabled){position=node.scrollTop;userStorage.setItem(options.key,String(position))}};
 node.addEventListener('scroll',save);
 restore();
 return{
  update(next:typeof options){const wasEnabled=options.enabled;options=next;if(!wasEnabled&&options.enabled)restore()},
  destroy(){node.removeEventListener('scroll',save)},
 };
}
