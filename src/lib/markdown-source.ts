export function splitFrontmatter(body:string):{prefix:string;body:string}{const match=body.match(/^(---\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)(?:\r?\n|$))/);return match?{prefix:match[0],body:body.slice(match[0].length)}:{prefix:'',body};}
export function protectHtml(body:string,renderContent=false):string{
 const lines=body.split('\n'),out:string[]=[];let fence='';
 for(let i=0;i<lines.length;i++){
  const line=lines[i],match=line.match(/^\s{0,3}(`{3,}|~{3,})/);
  if(match){if(!fence)fence=match[1];else if(match[1][0]===fence[0]&&match[1].length>=fence.length)fence='';out.push(line);continue;}
  const content=!fence&&renderContent?line.match(/^ {0,3}<(table|p|h[1-6]|a|img)(?:\s|>)/i):null;
  if(content){
   out.push(line);
   const closing=new RegExp(`</${content[1]}\\s*>`,'i');
   if(content[1].toLowerCase()!=='img')while(!closing.test(lines[i])&&i+1<lines.length)out.push(lines[++i]);
   continue;
  }
  if(!fence&&/^\s*<[^>]+>/.test(line)){const raw=[line];while(i+1<lines.length&&/^\s*</.test(lines[i+1]))raw.push(lines[++i]);out.push('````sag-raw',...raw,'````');}
  else out.push(line);
 }
 return out.join('\n');
}
export function restoreHtml(body:string):string{return body.replace(/^(`{3,})sag-raw\n([\s\S]*?)\n\1[ \t]*$/gm,'$2');}
