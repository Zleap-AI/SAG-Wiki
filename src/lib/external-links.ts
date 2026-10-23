export function externalLinkClick(event:MouseEvent):void{
 if(event.defaultPrevented)return;
 const link=(event.target as Element).closest<HTMLAnchorElement>('a[href]');
 if(!link||link.getAttribute('href')!.startsWith('#')||!/^(https?:|mailto:)/i.test(link.href))return;
 event.preventDefault();
 window.dispatchEvent(new CustomEvent('open-external-link',{detail:link.href}));
}
