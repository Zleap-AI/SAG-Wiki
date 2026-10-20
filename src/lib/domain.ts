import type {Candidate,Entry,FileInfo} from './types';
import {get} from 'svelte/store';
import {t} from './i18n';
const compactFormatter=new Intl.NumberFormat('en-US',{notation:'compact',maximumFractionDigits:1});
export const compactNumber=(value:number)=>compactFormatter.format(value);
export function durationLabel(milliseconds:number):string{const seconds=milliseconds/1000;if(seconds<60)return `${Math.round(seconds*10)/10}s`;const rounded=Math.round(seconds);return `${Math.floor(rounded/60)}m ${rounded%60}s`;}
export const basename=(path:string)=>path.split(/[\\/]/).filter(Boolean).pop()||path;
export const dirname=(path:string)=>path.replace(/[\\/][^\\/]+[\\/]?$/,'');
export function displayPath(path:string):string{
 if(path.startsWith('\\\\?\\UNC\\'))return '\\\\'+path.slice(8);
 return path.replace(/^\\\\\?\\([A-Za-z]:\\)/,'$1');
}
export const inside=(path:string,parent:string)=>path===parent||path.startsWith(parent.replace(/[\\/]$/,'')+(parent.includes('\\')?'\\':'/'));
export function indexedDirectoryEntries(files:FileInfo[],directory:string):Entry[]{
 const prefix=directory.replace(/[\\/]$/,'')+(directory.includes('\\')?'\\':'/');
 const entries=new Map<string,Entry>();
 for(const file of files){
  if(!file.path.startsWith(prefix))continue;
  const relative=file.path.slice(prefix.length),separator=relative.search(/[\\/]/);
  const name=separator<0?relative:relative.slice(0,separator),path=prefix+name;
  entries.set(path,{path,name,directory:separator>=0,symlink:false});
 }
 return [...entries.values()].sort((a,b)=>Number(b.directory)-Number(a.directory)||a.name.localeCompare(b.name));
}
export function normalizeScope(paths:string[]):string[]{return [...new Set(paths)].filter(p=>!paths.some(other=>other!==p&&inside(p,other)));}
export function documentResults(candidates:Candidate[]):{first:Candidate;matches:Candidate[]}[]{const groups=new Map<string,Candidate[]>();for(const c of candidates){const list=groups.get(c.path)||[];list.push(c);groups.set(c.path,list)}return [...groups.values()].map(matches=>({first:matches[0],matches}));}
export const escaped=(text:string)=>text.replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
export function sourceHtml(text:string,evidence:Candidate[]):string{return escaped(text).replace(/\[(\d+)\]/g,(raw,n)=>evidence[Number(n)-1]?`<button class="citation" data-citation="${Number(n)-1}">${n}</button>`:raw).replace(/\n/g,'<br>');}
export function fileName(value:string):string{let n=value.trim();if(!n.toLowerCase().endsWith('.md'))n+='.md';if(!n||/[\\/:\0]/.test(n)||/[ .]$/.test(n)||/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(n))throw Error('请输入有效的 Markdown 文件名');return n;}

export const isMarkdown=(path:string)=>/\.(md|markdown)$/i.test(path);
export const isRtf=(path:string)=>/\.rtf$/i.test(path);
export const isHtml=(path:string)=>/\.html?$/i.test(path);
export const readonlyHtml=(path:string,body:string)=>`<pre class="${isRtf(path)?'rtf-preview':'code-preview'}">${escaped(body)}</pre>`;
export function sourceLocation(c:Candidate):string {const translate=get(t);const location=c.source_spans?.length?c.source_spans.map(s=>translate('第 {value0} 行 {value1} 列 → 第 {value2} 行 {value3} 列',{value0:s.start_line,value1:s.start_column,value2:s.end_line,value3:s.end_column})).join(translate('；')):translate('第 {value0}–{value1} 行',{value0:c.start_line,value1:c.end_line});return isRtf(c.path)?`${translate('RTF 正文')} · ${location}`:location;}
