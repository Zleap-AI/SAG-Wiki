let preference='light';

export function applyTheme(theme:string){
 preference=theme;
 document.documentElement.dataset.theme=theme==='system'
  ?(window.matchMedia('(prefers-color-scheme: dark)').matches?'dark':'light')
  :theme;
}

export function watchSystemTheme(){
 const media=window.matchMedia('(prefers-color-scheme: dark)');
 const update=()=>{if(preference==='system')applyTheme(preference)};
 media.addEventListener('change',update);
 return ()=>media.removeEventListener('change',update);
}
