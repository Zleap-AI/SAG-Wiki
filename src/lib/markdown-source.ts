export function splitFrontmatter(body:string):{prefix:string;body:string}{const match=body.match(/^(---\r?\n[\s\S]*?\r?\n(?:---|\.\.\.)(?:\r?\n|$))/);return match?{prefix:match[0],body:body.slice(match[0].length)}:{prefix:'',body};}
export function protectHtml(body:string):string{
 const lines=body.split('\n'),out:string[]=[];let fence='';
 for(let i=0;i<lines.length;i++){
  const line=lines[i],match=line.match(/^\s{0,3}(`{3,}|~{3,})/);
  if(match){if(!fence)fence=match[1];else if(match[1][0]===fence[0]&&match[1].length>=fence.length)fence='';out.push(line);continue;}
  if(!fence&&/^\s*<[^>]+>/.test(line)){const raw=[line];while(i+1<lines.length&&/^\s*</.test(lines[i+1]))raw.push(lines[++i]);out.push('````sag-raw',...raw,'````');}
  else out.push(line);
 }
 return out.join('\n');
}
export function restoreHtml(body:string):string{return body.replace(/^(`{3,})sag-raw\n([\s\S]*?)\n\1[ \t]*$/gm,'$2');}
