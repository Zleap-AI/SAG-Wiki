import Defuddle from 'defuddle/full';
const result = new Defuddle(document.cloneNode(true), {url:location.href, markdown:true, useAsync:false}).parse();
globalThis.__sagWebClip = {title:result.title || document.title, url:location.href, content:result.content};
